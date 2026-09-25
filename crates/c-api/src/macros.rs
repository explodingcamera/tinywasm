macro_rules! export {
    ($(#[$meta:meta])* pub unsafe extern "C" fn $name:ident($($args:tt)*) $(-> $result:ty)? $body:block) => {
        $(#[$meta])*
        #[cfg_attr(not(feature = "custom-prefix"), unsafe(no_mangle))]
        #[cfg_attr(feature = "custom-prefix", unsafe(export_name = concat!(env!("TINYWASM_C_API_PREFIX"), stringify!($name))))]
        pub unsafe extern "C" fn $name($($args)*) $(-> $result)? $body
    };
}
