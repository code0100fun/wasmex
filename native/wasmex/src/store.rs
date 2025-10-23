use crate::{
    caller::{get_caller, get_caller_mut},
    engine::{unwrap_engine, EngineResource},
    pipe::{Pipe, PipeResource},
};
use rustler::{Error, NifStruct, ResourceArc};
use std::{collections::HashMap, sync::Mutex};
use wasi_common::sync::WasiCtxBuilder;
use wasmtime::{
    AsContext, AsContextMut, Engine, Store, StoreContext, StoreContextMut, StoreLimits,
    StoreLimitsBuilder,
};
use wasmtime_wasi::p2::pipe::{MemoryInputPipe, MemoryOutputPipe};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

#[derive(Debug, NifStruct)]
#[module = "Wasmex.Wasi.PreopenOptions"]
pub struct ExWasiPreopenOptions {
    path: String,
    alias: Option<String>,
}

#[derive(NifStruct)]
#[module = "Wasmex.Pipe"]
pub struct ExPipe {
    pub resource: ResourceArc<PipeResource>,
}

#[derive(NifStruct)]
#[module = "Wasmex.Wasi.WasiOptions"]
pub struct ExWasiOptions {
    args: Vec<String>,
    env: HashMap<String, String>,
    stderr: Option<ExPipe>,
    stdin: Option<ExPipe>,
    stdout: Option<ExPipe>,
    preopen: Vec<ExWasiPreopenOptions>,
}

#[derive(NifStruct)]
#[module = "Wasmex.Wasi.WasiP2Options"]
pub struct ExWasiP2Options {
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub stdin: Option<ExPipe>,
    pub stdout: Option<ExPipe>,
    pub stderr: Option<ExPipe>,
    pub inherit_stdin: bool,
    pub inherit_stdout: bool,
    pub inherit_stderr: bool,
    pub allow_http: bool,
}

#[derive(NifStruct)]
#[module = "Wasmex.StoreLimits"]
pub struct ExStoreLimits {
    memory_size: Option<usize>,
    table_elements: Option<usize>,
    instances: Option<usize>,
    tables: Option<usize>,
    memories: Option<usize>,
}

impl ExStoreLimits {
    pub fn to_wasmtime(&self) -> StoreLimits {
        let limits = StoreLimitsBuilder::new();

        let limits = if let Some(memory_size) = self.memory_size {
            limits.memory_size(memory_size)
        } else {
            limits
        };

        let limits = if let Some(table_elements) = self.table_elements {
            limits.table_elements(table_elements)
        } else {
            limits
        };

        let limits = if let Some(instances) = self.instances {
            limits.instances(instances)
        } else {
            limits
        };

        let limits = if let Some(tables) = self.tables {
            limits.tables(tables)
        } else {
            limits
        };

        let limits = if let Some(memories) = self.memories {
            limits.memories(memories)
        } else {
            limits
        };

        limits.build()
    }
}

pub struct StoreData {
    pub(crate) wasi: Option<wasi_common::WasiCtx>,
    pub(crate) limits: StoreLimits,
}

pub struct ComponentStoreData {
    pub(crate) ctx: Option<WasiCtx>,
    pub(crate) http: Option<WasiHttpCtx>,
    pub(crate) limits: StoreLimits,
    pub(crate) table: ResourceTable,
    // Infrastructure for Phase 3: reading captured stdout/stderr from pipes
    #[allow(dead_code)]
    pub(crate) stdout_pipe: Option<MemoryOutputPipe>,
    #[allow(dead_code)]
    pub(crate) stderr_pipe: Option<MemoryOutputPipe>,
    #[allow(dead_code)]
    pub(crate) stdout_user_pipe: Option<ResourceArc<PipeResource>>,
    #[allow(dead_code)]
    pub(crate) stderr_user_pipe: Option<ResourceArc<PipeResource>>,
}

impl WasiHttpView for ComponentStoreData {
    fn ctx(&mut self) -> &mut WasiHttpCtx {
        self.http.as_mut().expect("WasiHttpCtx is not set")
    }

    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

impl WasiView for ComponentStoreData {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: self.ctx.as_mut().expect("WasiCtx is not set"),
            table: &mut self.table,
        }
    }
}

pub enum StoreOrCaller {
    Store(Store<StoreData>),
    Caller(i32),
}

pub struct StoreOrCallerResource {
    pub inner: Mutex<StoreOrCaller>,
}

pub struct ComponentStoreResource {
    pub inner: Mutex<Store<ComponentStoreData>>,
}

#[rustler::resource_impl()]
impl rustler::Resource for ComponentStoreResource {}

#[rustler::resource_impl()]
impl rustler::Resource for StoreOrCallerResource {}

impl StoreOrCaller {
    pub fn engine(&self) -> &Engine {
        match self {
            StoreOrCaller::Store(store) => store.engine(),
            StoreOrCaller::Caller(token) => get_caller(token).unwrap().engine(),
        }
    }

    pub fn data(&self) -> &StoreData {
        match self {
            StoreOrCaller::Store(store) => store.data(),
            StoreOrCaller::Caller(token) => get_caller(token).unwrap().data(),
        }
    }
}

impl AsContext for StoreOrCaller {
    type Data = StoreData;

    fn as_context(&self) -> StoreContext<'_, Self::Data> {
        match self {
            StoreOrCaller::Store(store) => store.as_context(),
            StoreOrCaller::Caller(token) => get_caller(token).unwrap().as_context(),
        }
    }
}

impl AsContextMut for StoreOrCaller {
    fn as_context_mut(&mut self) -> StoreContextMut<'_, Self::Data> {
        match self {
            StoreOrCaller::Store(store) => store.as_context_mut(),
            StoreOrCaller::Caller(token) => get_caller_mut(token).unwrap().as_context_mut(),
        }
    }
}

#[rustler::nif(name = "store_new")]
pub fn new(
    limits: Option<ExStoreLimits>,
    engine_resource: ResourceArc<EngineResource>,
) -> Result<ResourceArc<StoreOrCallerResource>, rustler::Error> {
    let engine = unwrap_engine(engine_resource)?;
    let limits = if let Some(limits) = limits {
        limits.to_wasmtime()
    } else {
        StoreLimits::default()
    };
    let mut store = Store::new(&engine, StoreData { wasi: None, limits });
    store.limiter(|state| &mut state.limits);
    let resource = ResourceArc::new(StoreOrCallerResource {
        inner: Mutex::new(StoreOrCaller::Store(store)),
    });
    Ok(resource)
}

#[rustler::nif(name = "component_store_new")]
pub fn component_store_new(
    limits: Option<ExStoreLimits>,
    engine_resource: ResourceArc<EngineResource>,
) -> Result<ResourceArc<ComponentStoreResource>, rustler::Error> {
    let engine = unwrap_engine(engine_resource)?;
    let limits = if let Some(limits) = limits {
        limits.to_wasmtime()
    } else {
        StoreLimits::default()
    };

    // Initialize a default WasiCtx for backward compatibility.
    // Components may use WASI P2 stdio even without explicit WASI options.
    let ctx = WasiCtx::builder().build();

    let mut store = Store::new(
        &engine,
        ComponentStoreData {
            http: None,
            ctx: Some(ctx),
            limits,
            table: wasmtime_wasi::ResourceTable::new(),
            stdout_pipe: None,
            stderr_pipe: None,
            stdout_user_pipe: None,
            stderr_user_pipe: None,
        },
    );
    store.limiter(|state| &mut state.limits);
    let resource: ResourceArc<ComponentStoreResource> = ResourceArc::new(ComponentStoreResource {
        inner: Mutex::new(store),
    });
    Ok(resource)
}

#[rustler::nif(name = "component_store_new_wasi")]
pub fn component_store_new_wasi(
    options: ExWasiP2Options,
    limits: Option<ExStoreLimits>,
    engine_resource: ResourceArc<EngineResource>,
) -> Result<ResourceArc<ComponentStoreResource>, rustler::Error> {
    let mut wasi_ctx_builder = WasiCtx::builder();

    for arg in &options.args {
        wasi_ctx_builder.arg(arg);
    }

    for (key, value) in &options.env {
        wasi_ctx_builder.env(key, value);
    }

    // Handle stdin: pipe takes precedence over inherit
    if let Some(_stdin_pipe) = &options.stdin {
        // For stdin, we'd need to copy data from user's Pipe to MemoryInputPipe
        // For now, just create an empty MemoryInputPipe
        let memory_pipe = MemoryInputPipe::new(vec![]);
        wasi_ctx_builder.stdin(memory_pipe);
    } else if options.inherit_stdin {
        wasi_ctx_builder.inherit_stdin();
    }

    // Handle stdout: pipe takes precedence over inherit
    let (stdout_pipe_ref, stdout_user_pipe_ref) = if let Some(stdout_pipe) = &options.stdout {
        let memory_pipe = MemoryOutputPipe::new(usize::MAX);
        let pipe_clone = memory_pipe.clone();
        wasi_ctx_builder.stdout(memory_pipe);
        (Some(pipe_clone), Some(stdout_pipe.resource.clone()))
    } else {
        if options.inherit_stdout {
            wasi_ctx_builder.inherit_stdout();
        }
        (None, None)
    };

    // Handle stderr: pipe takes precedence over inherit
    let (stderr_pipe_ref, stderr_user_pipe_ref) = if let Some(stderr_pipe) = &options.stderr {
        let memory_pipe = MemoryOutputPipe::new(usize::MAX);
        let pipe_clone = memory_pipe.clone();
        wasi_ctx_builder.stderr(memory_pipe);
        (Some(pipe_clone), Some(stderr_pipe.resource.clone()))
    } else {
        if options.inherit_stderr {
            wasi_ctx_builder.inherit_stderr();
        }
        (None, None)
    };

    if options.allow_http {
        wasi_ctx_builder.allow_ip_name_lookup(true);
    }

    let engine = unwrap_engine(engine_resource)?;
    let limits = if let Some(limits) = limits {
        limits.to_wasmtime()
    } else {
        StoreLimits::default()
    };

    let http_option = if options.allow_http {
        Some(WasiHttpCtx::new())
    } else {
        None
    };

    let mut store = Store::new(
        &engine,
        ComponentStoreData {
            ctx: Some(wasi_ctx_builder.build()),
            limits,
            http: http_option,
            table: wasmtime_wasi::ResourceTable::new(),
            stdout_pipe: stdout_pipe_ref,
            stderr_pipe: stderr_pipe_ref,
            stdout_user_pipe: stdout_user_pipe_ref,
            stderr_user_pipe: stderr_user_pipe_ref,
        },
    );
    store.limiter(|state| &mut state.limits);
    let resource: ResourceArc<ComponentStoreResource> = ResourceArc::new(ComponentStoreResource {
        inner: Mutex::new(store),
    });
    Ok(resource)
}

#[rustler::nif(name = "store_new_wasi")]
pub fn new_wasi(
    options: ExWasiOptions,
    limits: Option<ExStoreLimits>,
    engine_resource: ResourceArc<EngineResource>,
) -> Result<ResourceArc<StoreOrCallerResource>, rustler::Error> {
    let wasi_env = &options
        .env
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect::<Vec<_>>();

    let mut builder = WasiCtxBuilder::new();

    builder
        .args(&options.args)
        .map_err(|err| Error::Term(Box::new(err.to_string())))?
        .envs(wasi_env)
        .map_err(|err| Error::Term(Box::new(err.to_string())))?;

    add_pipe(options.stdin, &mut builder, |pipe, builder| {
        builder.stdin(pipe);
    })?;
    add_pipe(options.stdout, &mut builder, |pipe, builder| {
        builder.stdout(pipe);
    })?;
    add_pipe(options.stderr, &mut builder, |pipe, builder| {
        builder.stderr(pipe);
    })?;
    wasi_preopen_directories(options.preopen, &mut builder)?;
    let wasi_ctx = builder.build();

    let engine = unwrap_engine(engine_resource)?;
    let limits = if let Some(limits) = limits {
        limits.to_wasmtime()
    } else {
        StoreLimits::default()
    };
    let mut store = Store::new(
        &engine,
        StoreData {
            wasi: Some(wasi_ctx),
            limits,
        },
    );
    store.limiter(|state| &mut state.limits);
    let resource = ResourceArc::new(StoreOrCallerResource {
        inner: Mutex::new(StoreOrCaller::Store(store)),
    });
    Ok(resource)
}

#[rustler::nif(name = "store_or_caller_set_fuel")]
pub fn set_fuel(
    store_or_caller_resource: ResourceArc<StoreOrCallerResource>,
    fuel: u64,
) -> Result<(), rustler::Error> {
    let store_or_caller: &mut StoreOrCaller =
        &mut *(store_or_caller_resource.inner.try_lock().map_err(|e| {
            rustler::Error::Term(Box::new(format!("Could not unlock store resource: {e}")))
        })?);
    match store_or_caller {
        StoreOrCaller::Store(store) => store.set_fuel(fuel),
        StoreOrCaller::Caller(token) => get_caller_mut(token)
            .ok_or_else(|| {
                rustler::Error::Term(Box::new(
                    "Caller is not valid. Only use a caller within its own function scope.",
                ))
            })
            .map(|c| c.set_fuel(fuel))?,
    }
    .map_err(|e| rustler::Error::Term(Box::new(format!("Could not set fuel: {e}"))))
}

#[rustler::nif(name = "store_or_caller_get_fuel")]
pub fn get_fuel(
    store_or_caller_resource: ResourceArc<StoreOrCallerResource>,
) -> Result<u64, rustler::Error> {
    let store_or_caller: &mut StoreOrCaller =
        &mut *(store_or_caller_resource.inner.try_lock().map_err(|e| {
            rustler::Error::Term(Box::new(format!("Could not unlock store resource: {e}")))
        })?);
    match store_or_caller {
        StoreOrCaller::Store(store) => store.get_fuel(),
        StoreOrCaller::Caller(token) => get_caller_mut(token)
            .ok_or_else(|| {
                rustler::Error::Term(Box::new(
                    "Caller is not valid. Only use a caller within its own function scope.",
                ))
            })
            .map(|c| c.get_fuel())?,
    }
    .map_err(|e| rustler::Error::Term(Box::new(format!("Could not get fuel: {e}"))))
}

fn add_pipe(
    pipe: Option<ExPipe>,
    builder: &mut WasiCtxBuilder,
    f: fn(Box<Pipe>, &mut WasiCtxBuilder) -> (),
) -> Result<(), rustler::Error> {
    if let Some(ExPipe { resource }) = pipe {
        let pipe = resource.pipe.lock().map_err(|_e| {
            rustler::Error::Term(Box::new(
                "Could not unlock resource as the mutex was poisoned.",
            ))
        })?;
        let pipe = Box::new(pipe.clone());
        f(pipe, builder);
    }
    Ok(())
}

fn wasi_preopen_directories(
    preopens: Vec<ExWasiPreopenOptions>,
    builder: &mut WasiCtxBuilder,
) -> Result<(), rustler::Error> {
    preopens
        .iter()
        .try_fold((), |_acc, preopen| preopen_directory(builder, preopen))
}

fn preopen_directory(
    builder: &mut WasiCtxBuilder,
    preopen: &ExWasiPreopenOptions,
) -> Result<(), Error> {
    let path = &preopen.path;
    let dir = wasi_common::sync::Dir::from_std_file(
        std::fs::File::open(path).map_err(|err| rustler::Error::Term(Box::new(err.to_string())))?,
    );
    let guest_path = preopen.alias.as_ref().unwrap_or(path);
    builder
        .preopened_dir(dir, guest_path)
        .map_err(|err| Error::Term(Box::new(err.to_string())))?;
    Ok(())
}

/// Syncs captured stdout/stderr from MemoryOutputPipe to user's Pipe resources.
/// This should be called after component execution to make captured output available.
/// Returns (stdout_bytes_written, stderr_bytes_written).
#[derive(rustler::NifTuple)]
struct PipeSyncResult {
    stdout_bytes: usize,
    stderr_bytes: usize,
}

#[rustler::nif(name = "component_sync_pipe_output", schedule = "DirtyCpu")]
pub fn component_sync_pipe_output(
    component_store_resource: ResourceArc<ComponentStoreResource>,
) -> Result<PipeSyncResult, rustler::Error> {
    use std::io::Write;

    let mut component_store = component_store_resource.inner.lock().map_err(|e| {
        rustler::Error::Term(Box::new(format!("Could not unlock store resource: {e}")))
    })?;

    let mut stdout_bytes_written = 0;
    let mut stderr_bytes_written = 0;

    // Take ownership of the pipes from the store (replacing with None)
    // This allows us to consume them with try_into_inner()
    let (stdout_memory_pipe, stdout_user_pipe, stderr_memory_pipe, stderr_user_pipe) = {
        let store_data = component_store.data_mut();
        (
            store_data.stdout_pipe.take(),
            store_data.stdout_user_pipe.clone(), // Keep user pipe reference
            store_data.stderr_pipe.take(),
            store_data.stderr_user_pipe.clone(), // Keep user pipe reference
        )
    };

    // Sync stdout if both pipes are present
    if let (Some(memory_pipe), Some(user_pipe)) = (stdout_memory_pipe, stdout_user_pipe) {
        // Get the captured bytes
        let bytes = memory_pipe.contents();

        // Write to user's pipe
        let mut pipe = user_pipe.pipe.lock().map_err(|e| {
            rustler::Error::Term(Box::new(format!("Could not unlock stdout pipe: {e}")))
        })?;

        stdout_bytes_written = pipe.write(&bytes).map_err(|e| {
            rustler::Error::Term(Box::new(format!("Failed to write to stdout pipe: {e}")))
        })?;
    }

    // Sync stderr if both pipes are present
    if let (Some(memory_pipe), Some(user_pipe)) = (stderr_memory_pipe, stderr_user_pipe) {
        // Get the captured bytes
        let bytes = memory_pipe.contents();

        // Write to user's pipe
        let mut pipe = user_pipe.pipe.lock().map_err(|e| {
            rustler::Error::Term(Box::new(format!("Could not unlock stderr pipe: {e}")))
        })?;

        stderr_bytes_written = pipe.write(&bytes).map_err(|e| {
            rustler::Error::Term(Box::new(format!("Failed to write to stderr pipe: {e}")))
        })?;
    }

    Ok(PipeSyncResult {
        stdout_bytes: stdout_bytes_written,
        stderr_bytes: stderr_bytes_written,
    })
}
