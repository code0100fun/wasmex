use rustler::{Encoder, NifResult, Term};
use wit_parser::{Resolve, TypeDefKind, WorldItem};

#[rustler::nif(name = "wit_exported_functions")]
pub fn exported_functions(env: rustler::Env, path: String, wit: String) -> NifResult<Term> {
    let mut resolve = Resolve::new();
    let id = resolve
        .push_str(path, &wit)
        .map_err(|e| rustler::Error::Term(Box::new(format!("Failed to parse WIT: {e}"))))?;
    let world_id = resolve
        .select_world(&[id], None)
        .map_err(|e| rustler::Error::Term(Box::new(format!("Failed to select world: {e}"))))?;
    let exports = &resolve.worlds[world_id].exports;
    let exported_functions = exports
        .iter()
        .filter_map(|(_key, value)| match value {
            WorldItem::Function(function) => Some((&function.name, function.params.len())),
            _ => None,
        })
        .collect::<Vec<(&String, usize)>>();
    Ok(Term::map_from_pairs(env, exported_functions.as_slice()).unwrap())
}

/// Extract detailed component metadata including function signatures
#[rustler::nif(name = "component_metadata")]
pub fn component_metadata<'a>(
    env: rustler::Env<'a>,
    wasm_bytes: rustler::Binary,
) -> NifResult<Term<'a>> {
    use wit_parser::decoding::{decode, DecodedWasm};

    let bytes = wasm_bytes.as_slice();
    let decoded = decode(bytes)
        .map_err(|e| rustler::Error::Term(Box::new(format!("Failed to decode WASM: {e}"))))?;

    let (resolve, world_id) = match decoded {
        DecodedWasm::Component(r, w) => (r, w),
        DecodedWasm::WitPackage(_, _) => {
            return Err(rustler::Error::Term(Box::new(
                "Expected a component, not a WIT package",
            )))
        }
    };

    let world = &resolve.worlds[world_id];

    // Extract exported functions with full signatures
    let mut exported_functions = Vec::new();
    for (_key, item) in &world.exports {
        match item {
            // Direct function export at world level
            WorldItem::Function(func) => {
                // Build parameter list with names and types
                let params: Vec<(String, String)> = func
                    .params
                    .iter()
                    .map(|(name, ty)| (name.clone(), type_to_string(&resolve, ty)))
                    .collect();

                // Get return type
                let returns = match &func.result {
                    Some(ty) => type_to_string(&resolve, ty),
                    None => String::new(),
                };

                // Direct exports don't have an interface - use empty string
                let interface_name = String::new();

                exported_functions.push((func.name.clone(), params, returns, interface_name));
            }
            // Interface export - extract functions from the interface
            WorldItem::Interface { id, .. } => {
                let interface = &resolve.interfaces[*id];
                // Get the fully qualified interface name
                let interface_name = interface_to_qualified_name(&resolve, *id);

                // Extract all functions from this interface
                for (_func_name, func) in &interface.functions {
                    let params: Vec<(String, String)> = func
                        .params
                        .iter()
                        .map(|(name, ty)| (name.clone(), type_to_string(&resolve, ty)))
                        .collect();

                    let returns = match &func.result {
                        Some(ty) => type_to_string(&resolve, ty),
                        None => String::new(),
                    };

                    exported_functions.push((
                        func.name.clone(),
                        params,
                        returns,
                        interface_name.clone(),
                    ));
                }
            }
            // Ignore other types (Type exports, etc.)
            _ => {}
        }
    }

    // Build result list
    let functions_list: Vec<Term> = exported_functions
        .iter()
        .map(|(name, params, returns, interface)| {
            // Build parameters list
            let params_list: Vec<Term> = params
                .iter()
                .map(|(pname, ptype)| {
                    let param_map = rustler::types::map::map_new(env);
                    let param_map = param_map
                        .map_put("name".encode(env), pname.encode(env))
                        .unwrap();
                    let param_map = param_map
                        .map_put("type".encode(env), ptype.encode(env))
                        .unwrap();
                    param_map.encode(env)
                })
                .collect();

            // Build function map
            let func_map = rustler::types::map::map_new(env);
            let func_map = func_map
                .map_put("name".encode(env), name.encode(env))
                .unwrap();
            let func_map = func_map
                .map_put("params".encode(env), params_list.encode(env))
                .unwrap();
            let func_map = func_map
                .map_put("returns".encode(env), returns.encode(env))
                .unwrap();
            let func_map = func_map
                .map_put("interface".encode(env), interface.encode(env))
                .unwrap();
            func_map.encode(env)
        })
        .collect();

    Ok(functions_list.encode(env))
}

/// Get the fully qualified name for an interface
fn interface_to_qualified_name(resolve: &Resolve, interface_id: wit_parser::InterfaceId) -> String {
    let interface = &resolve.interfaces[interface_id];

    // Get the package and interface name
    if let Some(package_id) = interface.package {
        let package = &resolve.packages[package_id];
        let package_name = &package.name;

        // Format as namespace:package/interface@version
        if let Some(iface_name) = &interface.name {
            format!(
                "{}:{}/{}@{}",
                package_name.namespace,
                package_name.name,
                iface_name,
                package_name
                    .version
                    .as_ref()
                    .map(|v| format!("{}.{}.{}", v.major, v.minor, v.patch))
                    .unwrap_or_else(|| "0.0.0".to_string())
            )
        } else {
            format!("{}:{}", package_name.namespace, package_name.name)
        }
    } else {
        // No package, just use the interface name if available
        interface
            .name
            .clone()
            .unwrap_or_else(|| "unknown".to_string())
    }
}

/// Convert a WIT type to a string representation
fn type_to_string(resolve: &Resolve, ty: &wit_parser::Type) -> String {
    match ty {
        wit_parser::Type::Bool => "bool".to_string(),
        wit_parser::Type::U8 => "u8".to_string(),
        wit_parser::Type::U16 => "u16".to_string(),
        wit_parser::Type::U32 => "u32".to_string(),
        wit_parser::Type::U64 => "u64".to_string(),
        wit_parser::Type::S8 => "s8".to_string(),
        wit_parser::Type::S16 => "s16".to_string(),
        wit_parser::Type::S32 => "s32".to_string(),
        wit_parser::Type::S64 => "s64".to_string(),
        wit_parser::Type::F32 => "f32".to_string(),
        wit_parser::Type::F64 => "f64".to_string(),
        wit_parser::Type::Char => "char".to_string(),
        wit_parser::Type::String => "string".to_string(),
        wit_parser::Type::ErrorContext => "error-context".to_string(),
        wit_parser::Type::Id(id) => {
            let typedef = &resolve.types[*id];
            match &typedef.kind {
                TypeDefKind::List(inner) => format!("list<{}>", type_to_string(resolve, inner)),
                TypeDefKind::Option(inner) => format!("option<{}>", type_to_string(resolve, inner)),
                TypeDefKind::Result(r) => {
                    let ok_ty =
                        r.ok.as_ref()
                            .map(|t| type_to_string(resolve, t))
                            .unwrap_or_else(|| "_".to_string());
                    let err_ty = r
                        .err
                        .as_ref()
                        .map(|t| type_to_string(resolve, t))
                        .unwrap_or_else(|| "_".to_string());
                    format!("result<{}, {}>", ok_ty, err_ty)
                }
                TypeDefKind::Tuple(tuple) => {
                    let types: Vec<String> = tuple
                        .types
                        .iter()
                        .map(|t| type_to_string(resolve, t))
                        .collect();
                    format!("tuple<{}>", types.join(", "))
                }
                _ => typedef
                    .name
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            }
        }
    }
}
