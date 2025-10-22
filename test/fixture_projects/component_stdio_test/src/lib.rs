#[allow(warnings)]
mod bindings;

use bindings::Guest;

struct Component;

impl Guest for Component {
    fn print_hello() -> String {
        println!("Hello from stdout!");
        "printed to stdout".to_string()
    }

    fn print_error() -> String {
        eprintln!("Error from stderr!");
        "printed to stderr".to_string()
    }

    fn print_mixed() -> String {
        println!("This goes to stdout");
        eprintln!("This goes to stderr");
        println!("More stdout");
        "printed to both".to_string()
    }
}

const _: () = {
    #[export_name = "print-hello"]
    #[allow(non_snake_case)]
    unsafe extern "C" fn __export_print_hello() -> *mut u8 {
        bindings::_export_print_hello_cabi::<Component>()
    }

    #[export_name = "cabi_post_print-hello"]
    #[allow(non_snake_case)]
    unsafe extern "C" fn __export_post_return_print_hello(arg0: *mut u8) {
        bindings::__post_return_print_hello::<Component>(arg0)
    }

    #[export_name = "print-error"]
    #[allow(non_snake_case)]
    unsafe extern "C" fn __export_print_error() -> *mut u8 {
        bindings::_export_print_error_cabi::<Component>()
    }

    #[export_name = "cabi_post_print-error"]
    #[allow(non_snake_case)]
    unsafe extern "C" fn __export_post_return_print_error(arg0: *mut u8) {
        bindings::__post_return_print_error::<Component>(arg0)
    }

    #[export_name = "print-mixed"]
    #[allow(non_snake_case)]
    unsafe extern "C" fn __export_print_mixed() -> *mut u8 {
        bindings::_export_print_mixed_cabi::<Component>()
    }

    #[export_name = "cabi_post_print-mixed"]
    #[allow(non_snake_case)]
    unsafe extern "C" fn __export_post_return_print_mixed(arg0: *mut u8) {
        bindings::__post_return_print_mixed::<Component>(arg0)
    }
};
