use std::collections::HashMap;
use std::sync::{Condvar, Mutex};

use rustler::env::SavedTerm;
use wit_parser::{Function, Resolve, WorldItem};

use crate::atoms;
use crate::component::ComponentResource;
use crate::engine::{EngineResource, TOKIO_RUNTIME};
use crate::store::ComponentStoreData;
use crate::store::ComponentStoreResource;
use rustler::types::tuple::make_tuple;
use rustler::NifResult;
use rustler::ResourceArc;
use rustler::{Encoder, OwnedEnv};
use rustler::{Error, LocalPid};
use wasmtime::component::{Instance, Linker, LinkerInstance, Type, Val};
use wasmtime::Trap;
use wiggle::anyhow::{self};

use rustler::Term;

use wasmtime::Store;

use wasmtime_wasi;
use wasmtime_wasi_http::{self, WasiHttpView};

use crate::component_type_conversion::{
    convert_params, convert_result_term, encode_result, vals_to_terms,
};

pub struct ComponentCallbackToken {
    pub continue_signal: Condvar,
    pub name: String,
    pub namespace: Option<String>,
    pub return_values: Mutex<Option<(bool, Vec<Val>)>>,
}

pub struct ComponentCallbackTokenResource {
    pub token: ComponentCallbackToken,
}

#[rustler::resource_impl()]
impl rustler::Resource for ComponentCallbackTokenResource {}

pub struct ComponentInstanceResource {
    pub inner: Mutex<Instance>,
}

#[rustler::resource_impl()]
impl rustler::Resource for ComponentInstanceResource {}

// Resource for storing ProxyPre - used for fast HTTP handler instantiation
pub struct ProxyPreResource {
    pub inner: Mutex<wasmtime_wasi_http::bindings::ProxyPre<ComponentStoreData>>,
}

#[rustler::resource_impl()]
impl rustler::Resource for ProxyPreResource {}

#[rustler::nif(name = "component_instance_new")]
pub fn new_instance(
    store_resource: ResourceArc<ComponentStoreResource>,
    component_resource: ResourceArc<ComponentResource>,
    imports: rustler::Term,
) -> NifResult<ResourceArc<ComponentInstanceResource>> {
    let store: &mut Store<ComponentStoreData> =
        &mut *(store_resource.inner.lock().map_err(|e| {
            rustler::Error::Term(Box::new(format!(
                "Could not unlock store resource as the mutex was poisoned: {e}"
            )))
        })?);

    let component = component_resource.inner.lock().map_err(|e| {
        rustler::Error::Term(Box::new(format!(
            "Could not unlock component resource as the mutex was poisoned: {e}"
        )))
    })?;

    let mut linker = Linker::new(store.engine());
    linker.allow_shadowing(true);
    let _ = wasmtime_wasi::p2::add_to_linker_async(&mut linker);
    if store.data().http.is_some() {
        let _ = wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker);
    }

    // Instantiate the component

    // Handle imports
    let imports_map = imports.decode::<HashMap<String, Term>>()?;
    for (name, implementation) in imports_map {
        if Term::is_tuple(implementation) {
            // root imports
            link_import(&mut linker.root(), name, None, implementation)?;
        } else {
            let imports_map = implementation.decode::<HashMap<String, Term>>()?;
            let mut namespace = linker
                .instance(&name)
                .map_err(|e| rustler::Error::Term(Box::new(e.to_string())))?;
            for (implementation_name, implementation) in imports_map {
                link_import(
                    &mut namespace,
                    implementation_name,
                    Some(name.clone()),
                    implementation,
                )?;
            }
        }
    }

    // Use async instantiation with async linker
    // We block_on here since new_instance is a sync NIF called during setup (not hot path)
    let instance = TOKIO_RUNTIME
        .block_on(linker.instantiate_async(&mut *store, &component))
        .map_err(|e| rustler::Error::Term(Box::new(e.to_string())))?;

    Ok(ResourceArc::new(ComponentInstanceResource {
        inner: Mutex::new(instance),
    }))
}

/// Create a ProxyPre for fast HTTP handler instantiation
/// This pre-compiles the component and is used to create fresh instances per HTTP request
#[rustler::nif(name = "component_proxy_pre_new")]
pub fn new_proxy_pre(
    store_resource: ResourceArc<ComponentStoreResource>,
    component_resource: ResourceArc<ComponentResource>,
) -> NifResult<ResourceArc<ProxyPreResource>> {
    let store: &mut Store<ComponentStoreData> =
        &mut *(store_resource.inner.lock().map_err(|e| {
            rustler::Error::Term(Box::new(format!(
                "Could not unlock store resource as the mutex was poisoned: {e}"
            )))
        })?);

    let component = component_resource.inner.lock().map_err(|e| {
        rustler::Error::Term(Box::new(format!(
            "Could not unlock component resource as the mutex was poisoned: {e}"
        )))
    })?;

    // Create linker with WASI support
    let mut linker = Linker::new(store.engine());
    linker.allow_shadowing(true);
    let _ = wasmtime_wasi::p2::add_to_linker_async(&mut linker);

    // Only add HTTP support if configured
    if store.data().http.is_some() {
        let _ = wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker);
    }

    // Pre-instantiate the component (expensive but done once)
    let instance_pre = linker
        .instantiate_pre(&component)
        .map_err(|e| rustler::Error::Term(Box::new(e.to_string())))?;

    // Create ProxyPre from InstancePre
    let proxy_pre = wasmtime_wasi_http::bindings::ProxyPre::new(instance_pre)
        .map_err(|e| rustler::Error::Term(Box::new(e.to_string())))?;

    Ok(ResourceArc::new(ProxyPreResource {
        inner: Mutex::new(proxy_pre),
    }))
}

fn create_callback_token(
    name: String,
    namespace: Option<String>,
) -> ResourceArc<ComponentCallbackTokenResource> {
    ResourceArc::new(ComponentCallbackTokenResource {
        token: ComponentCallbackToken {
            continue_signal: Condvar::new(),
            name,
            namespace,
            return_values: Mutex::new(None),
        },
    })
}

fn call_elixir_import(
    name: String,
    namespace: Option<String>,
    params: &[Val],
    result_values: &mut [Val],
    pid: LocalPid,
) -> Result<(), anyhow::Error> {
    let mut msg_env = OwnedEnv::new();
    let callback_token = create_callback_token(name.clone(), namespace.clone());

    let _ = msg_env.send_and_clear(&pid, |env| {
        let param_terms = vals_to_terms(params, env);
        (
            atoms::invoke_callback(),
            namespace,
            name,
            callback_token.clone(),
            param_terms,
        )
    });

    let mut result = callback_token.token.return_values.lock().unwrap();
    while result.is_none() {
        result = callback_token.token.continue_signal.wait(result).unwrap();
    }

    let (success, returned_values) = result.take().unwrap();
    if !success {
        return Err(anyhow::anyhow!("Callback failed"));
    }

    if !returned_values.is_empty() {
        result_values[0] = returned_values[0].clone();
    }
    Ok(())
}

fn link_import(
    linker_instance: &mut LinkerInstance<ComponentStoreData>,
    name: String,
    namespace: Option<String>,
    implementation: Term,
) -> NifResult<()> {
    let pid = implementation.get_env().pid();
    let name_for_closure = name.clone();

    linker_instance
        .func_new(&name, move |_store, params, result_values| {
            call_elixir_import(
                name_for_closure.clone(),
                namespace.clone(),
                params,
                result_values,
                pid,
            )
        })
        .map_err(|e| rustler::Error::Term(Box::new(e.to_string())))
}

#[rustler::nif(name = "component_call_function")]
pub fn call_exported_function(
    component_store_resource: ResourceArc<ComponentStoreResource>,
    instance_resource: ResourceArc<ComponentInstanceResource>,
    function_name_path: Vec<String>,
    given_params: Term,
    from: Term,
) -> rustler::Atom {
    // create erlang environment for the thread
    let mut thread_env = OwnedEnv::new();
    // copy over params into the thread environment
    let function_params = thread_env.save(given_params);
    let from = thread_env.save(from);

    TOKIO_RUNTIME.spawn(async move {
        // Use spawn_blocking to run async wasmtime operations
        // This allows holding std::sync::MutexGuard across awaits
        // while using wasmtime's native async APIs
        let _ = tokio::task::spawn_blocking(move || {
            // Create a single-threaded runtime for this execution
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create runtime");

            let result = rt.block_on(component_execute_function_async(
                &mut thread_env,
                component_store_resource,
                instance_resource,
                function_name_path,
                function_params,
            ));

            // Send result directly to the caller
            thread_env.run(|env| {
                let from_tuple = from.load(env).decode::<Term>().unwrap();
                let result_term = result
                    .load(env)
                    .decode::<Term>()
                    .unwrap_or(atoms::error().encode(env));

                // GenServer.call from tuple is {pid, ref}
                // LocalPid in Rustler can handle both local and remote PIDs (despite the name)
                let (caller_pid, ref_term) = from_tuple
                    .decode::<(LocalPid, Term)>()
                    .expect("from must be a GenServer {pid, ref} tuple");

                // Send GenServer reply format directly to caller: {ref, result}
                let _ = env.send(&caller_pid, make_tuple(env, &[ref_term, result_term]));
            });
        })
        .await;
    });

    atoms::ok()
}

// We hold MutexGuard across await points intentionally here.
// This function runs inside spawn_blocking with a local runtime,
// so the standard async-aware Mutex isn't applicable.
#[allow(clippy::await_holding_lock)]
async fn component_execute_function_async(
    thread_env: &mut OwnedEnv,
    component_store_resource: ResourceArc<ComponentStoreResource>,
    instance_resource: ResourceArc<ComponentInstanceResource>,
    function_name_path: Vec<String>,
    function_params: SavedTerm,
) -> SavedTerm {
    // Step 1: Decode parameters inside thread_env.run()
    let given_params =
        thread_env.run(
            |env| match function_params.load(env).decode::<Vec<Term>>() {
                Ok(vec) => Ok(vec),
                Err(err) => Err(env
                    .error_tuple(format!("could not load 'function params': {err:?}"))
                    .encode(env)),
            },
        );

    let given_params = match given_params {
        Ok(params) => params,
        Err(err_term) => return thread_env.save(err_term),
    };

    // Step 2: Lock resources, find function, prepare for async call
    // We need to scope the locks to ensure they're dropped before the await
    let (function, param_types, results_count) = {
        let mut component_store = component_store_resource.inner.lock().unwrap();
        let instance = instance_resource.inner.lock().unwrap();

        // Find function by path
        let mut lookup_index = None;
        for (index, name) in function_name_path.iter().enumerate() {
            if let Some(inner) = lookup_index {
                lookup_index = instance
                    .get_export(&mut *component_store, Some(&inner), name.as_str())
                    .map(|(_, index)| index);
            } else {
                lookup_index = instance
                    .get_export(&mut *component_store, None, name.as_str())
                    .map(|(_, index)| index);
            }

            if lookup_index.is_none() {
                let err_msg = if function_name_path.len() == 1 {
                    format!(
                        "exported function `{}` not found.",
                        function_name_path.join(", ")
                    )
                } else {
                    format!(
                        "exported function `[{}]` not found. Could not find `{}` at position {}",
                        function_name_path.join(", "),
                        name,
                        index
                    )
                };
                return thread_env.run(|env| thread_env.save(env.error_tuple(err_msg).encode(env)));
            }
        }

        let lookup_index = match lookup_index {
            Some(index) => index,
            None => {
                let err_msg = format!(
                    "exported function `{}` not found.",
                    function_name_path.join(", ")
                );
                return thread_env.run(|env| thread_env.save(env.error_tuple(err_msg).encode(env)));
            }
        };

        let function = match instance.get_func(&mut *component_store, lookup_index) {
            Some(func) => func,
            None => {
                let err_msg = format!(
                    "exported function `{}` not found",
                    function_name_path.join(", ")
                );
                return thread_env.run(|env| thread_env.save(env.error_tuple(err_msg).encode(env)));
            }
        };

        let param_types = function.params(&*component_store);
        let param_types: Vec<Type> = param_types.as_ref().iter().map(|x| x.1.clone()).collect();
        let results_count = function.results(&*component_store).len();

        (function, param_types, results_count)
        // MutexGuards are dropped here, before any await
    };

    // Step 3: Convert parameters inside thread_env.run()
    let converted_params =
        thread_env.run(
            |env| match convert_params(param_types.as_ref(), given_params) {
                Ok(params) => Ok(params),
                Err(Error::Term(e)) => Err(env.error_tuple(e.encode(env)).encode(env)),
                Err(e) => {
                    let reason = format!("Error converting param: {e:?}");
                    Err(env.error_tuple(&reason).encode(env))
                }
            },
        );

    let converted_params = match converted_params {
        Ok(params) => params,
        Err(err_term) => return thread_env.save(err_term),
    };

    // Step 4: Re-lock and call function asynchronously
    // Lock scope: lock -> async call -> drop before encoding
    let result_vals = {
        let mut component_store = component_store_resource.inner.lock().unwrap();
        let mut result = vec![Val::Bool(false); results_count];

        match function
            .call_async(
                &mut *component_store,
                converted_params.as_slice(),
                &mut result,
            )
            .await
        {
            Ok(_) => {
                let _ = function.post_return_async(&mut *component_store).await;
                Ok(result)
            }
            Err(err) => Err(err),
        }
        // MutexGuard dropped here
    };

    // Step 5: Encode result inside thread_env.run()
    match result_vals {
        Ok(result) => thread_env.run(|env| thread_env.save(encode_result(env, result).encode(env))),
        Err(err) => {
            let reason = format!("{err}");
            let err_msg = if let Ok(trap) = err.downcast::<Trap>() {
                format!("Error during function excecution ({trap}): {reason}")
            } else {
                format!("Error during function excecution: {reason}")
            };
            thread_env.run(|env| thread_env.save(env.error_tuple(err_msg).encode(env)))
        }
    }
}

#[rustler::nif(name = "component_receive_callback_result")]
pub fn receive_callback_result(
    component_resource: ResourceArc<ComponentResource>,
    token_resource: ResourceArc<ComponentCallbackTokenResource>,
    _success: bool,
    result: Term,
) -> NifResult<rustler::Atom> {
    let parsed_component = &component_resource.parsed;
    let world = &parsed_component.resolve.worlds[parsed_component.world_id];
    let name = &token_resource.token.name;
    let namespace = &token_resource.token.namespace;

    let import_function = if let Some(namespace) = namespace {
        let (_package_name, _interface_name, interface_id) = parsed_component
            .resolve
            .package_names
            .iter()
            .flat_map(|(package_name, package_id)| {
                let package = parsed_component.resolve.packages.get(*package_id).unwrap();
                package
                    .interfaces
                    .iter()
                    .map(|(interface_name, interface_id)| {
                        (package_name.clone(), interface_name.clone(), *interface_id)
                    })
            })
            .find(|(package_name, interface_name, _interface_id)| {
                let namespace = namespace.to_string();
                let full_name = package_name.interface_id(interface_name);
                full_name == namespace
            })
            .ok_or_else(|| {
                Error::Term(Box::new(format!("Could not find package name {namespace}")))
            })?;
        let interface = parsed_component
            .resolve
            .interfaces
            .get(interface_id)
            .unwrap();
        let (_function_name, function) = interface
            .functions
            .iter()
            .find(|(function_name, _function)| function_name.as_str() == name)
            .ok_or_else(|| {
                Error::Term(Box::new(format!("Could not find import function {name}")))
            })?;
        function
    } else {
        world
            .imports
            .iter()
            .filter_map(|(_, item)| match item {
                WorldItem::Function(function) => Some(function),
                _ => None,
            })
            .find(|f| f.item_name() == name)
            .ok_or_else(|| {
                Error::Term(Box::new(format!("Could not find import function {name}")))
            })?
    };

    let return_values = token_resource
        .token
        .return_values
        .lock()
        .map_err(|e| Error::Term(Box::new(format!("Failed to lock return values: {e}"))))?;

    convert_return_values(
        &component_resource.parsed.resolve,
        import_function,
        return_values,
        result,
    )
    .map_err(|e| {
        Error::Term(Box::new(format!(
            "Failed to convert imported function return values - {e}"
        )))
    })?;

    token_resource.token.continue_signal.notify_one();

    Ok(atoms::ok())
}

/// Call an HTTP handler component using ProxyPre pattern
/// Creates a fresh Store for each request to avoid state pollution between requests
#[rustler::nif(name = "component_call_http_handler", schedule = "DirtyCpu")]
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub fn call_http_handler<'a>(
    env: rustler::Env<'a>,
    proxy_pre_resource: ResourceArc<ProxyPreResource>,
    engine_resource: ResourceArc<EngineResource>,
    wasi_options: crate::store::ExWasiP2Options,
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: rustler::Binary,
) -> NifResult<(u16, Vec<(String, String)>, rustler::Binary<'a>)> {
    use bytes::Bytes;
    use http_body_util::{BodyExt, Full};
    use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
    use wasmtime_wasi::{ResourceTable, WasiCtx};
    use wasmtime_wasi_http::bindings::http::types::Scheme;

    let body_bytes = body.as_slice().to_vec();

    // Use the global Tokio runtime
    TOKIO_RUNTIME.block_on(async {
        // Step 1: Create FRESH Store with new WASI context for this request
        // This is the key fix - each request gets clean state
        let engine = crate::engine::unwrap_engine(engine_resource)?;

        // Build WasiCtx from options
        let mut wasi_ctx_builder = WasiCtx::builder();

        for arg in &wasi_options.args {
            wasi_ctx_builder.arg(arg);
        }

        for (key, value) in &wasi_options.env {
            wasi_ctx_builder.env(key, value);
        }

        // Handle stdout/stderr pipes (similar to store.rs)
        let stdout_pipe = if wasi_options.stdout.is_some() {
            let pipe = MemoryOutputPipe::new(usize::MAX);
            let pipe_clone = pipe.clone();
            wasi_ctx_builder.stdout(pipe);
            Some(pipe_clone)
        } else if wasi_options.inherit_stdout {
            wasi_ctx_builder.inherit_stdout();
            None
        } else {
            None
        };

        let stderr_pipe = if wasi_options.stderr.is_some() {
            let pipe = MemoryOutputPipe::new(usize::MAX);
            let pipe_clone = pipe.clone();
            wasi_ctx_builder.stderr(pipe);
            Some(pipe_clone)
        } else if wasi_options.inherit_stderr {
            wasi_ctx_builder.inherit_stderr();
            None
        } else {
            None
        };

        let wasi_ctx = wasi_ctx_builder.build();

        // Create fresh HTTP context if needed
        let http_ctx = if wasi_options.allow_http {
            Some(wasmtime_wasi_http::WasiHttpCtx::new())
        } else {
            None
        };

        // Extract user pipes from options for sync_pipe_output
        let stdout_user_pipe = wasi_options.stdout.as_ref().map(|p| p.resource.clone());
        let stderr_user_pipe = wasi_options.stderr.as_ref().map(|p| p.resource.clone());

        // Create fresh Store with new state
        let mut store = Store::new(
            &engine,
            ComponentStoreData {
                http: http_ctx,
                ctx: Some(wasi_ctx),
                limits: wasmtime::StoreLimits::default(),
                table: ResourceTable::new(),
                stdout_pipe: stdout_pipe.clone(),
                stderr_pipe: stderr_pipe.clone(),
                stdout_user_pipe,
                stderr_user_pipe,
            },
        );
        store.limiter(|state| &mut state.limits);

        // Step 2: Instantiate proxy from ProxyPre (fast - microseconds!)
        // Clone ProxyPre out of mutex to avoid holding lock across await
        let proxy_pre = {
            let guard = proxy_pre_resource
                .inner
                .lock()
                .map_err(|e| Error::Term(Box::new(format!("Failed to lock ProxyPre: {e}"))))?;
            guard.clone()
        }; // MutexGuard dropped here

        let proxy = proxy_pre.instantiate_async(&mut store).await.map_err(|e| {
            Error::Term(Box::new(format!(
                "Failed to instantiate from ProxyPre: {e}"
            )))
        })?;

        // Step 3: Build HTTP request
        let mut request_builder = hyper::Request::builder()
            .method(method.as_str())
            .uri(&path)
            .header("host", "localhost");

        for (key, value) in headers {
            request_builder = request_builder.header(key, value);
        }

        let request = request_builder
            .body(
                Full::new(Bytes::from(body_bytes))
                    .map_err(|never| match never {})
                    .boxed(),
            )
            .map_err(|e| Error::Term(Box::new(format!("Failed to build request: {e}"))))?;

        // Create incoming request resource
        let incoming_req = store
            .data_mut()
            .new_incoming_request(Scheme::Http, request)
            .map_err(|e| {
                Error::Term(Box::new(format!("Failed to create incoming request: {e}")))
            })?;

        // Create response outparam with channel
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let response_out = store
            .data_mut()
            .new_response_outparam(sender)
            .map_err(|e| {
                Error::Term(Box::new(format!("Failed to create response outparam: {e}")))
            })?;

        // Step 4: Call the handler
        proxy
            .wasi_http_incoming_handler()
            .call_handle(&mut store, incoming_req, response_out)
            .await
            .map_err(|e| Error::Term(Box::new(format!("Handler call failed: {e}"))))?;

        // Step 5: Wait for response
        let response = receiver
            .await
            .map_err(|e| Error::Term(Box::new(format!("Failed to receive response: {e}"))))?
            .map_err(|e| Error::Term(Box::new(format!("Handler returned error: {e}"))))?;

        // Step 6: Process response
        let status = response.status().as_u16();

        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        let body_bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|e| Error::Term(Box::new(format!("Failed to read response body: {e}"))))?
            .to_bytes()
            .to_vec();

        // Step 7: Sync output from internal pipes to user pipes before Store is dropped
        {
            use std::io::Write;
            let store_data = store.data_mut();

            // Sync stdout if both pipes are present
            if let (Some(memory_pipe), Some(user_pipe)) = (
                store_data.stdout_pipe.as_ref(),
                store_data.stdout_user_pipe.as_ref(),
            ) {
                let bytes = memory_pipe.contents();
                if !bytes.is_empty() {
                    if let Ok(mut pipe) = user_pipe.pipe.lock() {
                        let _ = pipe.write(&bytes);
                    }
                }
            }

            // Sync stderr if both pipes are present
            if let (Some(memory_pipe), Some(user_pipe)) = (
                store_data.stderr_pipe.as_ref(),
                store_data.stderr_user_pipe.as_ref(),
            ) {
                let bytes = memory_pipe.contents();
                if !bytes.is_empty() {
                    if let Ok(mut pipe) = user_pipe.pipe.lock() {
                        let _ = pipe.write(&bytes);
                    }
                }
            }
        }

        // Convert to Elixir binary
        let mut binary = rustler::OwnedBinary::new(body_bytes.len()).unwrap();
        binary.as_mut_slice().copy_from_slice(&body_bytes);
        let body_binary = binary.release(env);

        Ok((status, headers, body_binary))
    })
}

fn convert_return_values(
    wit_resolver: &Resolve,
    function: &Function,
    mut return_values: std::sync::MutexGuard<'_, Option<(bool, Vec<Val>)>>,
    result: Term,
) -> Result<(), String> {
    if let Some(result_type) = &function.result {
        let mut vals = Vec::new();
        vals.push(
            convert_result_term(result, result_type, wit_resolver, vec![]).map_err(
                |(msg, path)| {
                    if path.is_empty() {
                        msg
                    } else {
                        format!("{msg:?} at path: {path:?}")
                    }
                },
            )?,
        );

        // Set the return values
        *return_values = Some((true, vals));
    } else {
        *return_values = Some((true, vec![]));
    }

    Ok(())
}
