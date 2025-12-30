#![feature(
    // allocator_api,
    // allocator_internals,
    ascii_char,
    ascii_char_variants,
    // associated_type_defaults,
    // async_for_loop,
    // async_iterator,
    // box_vec_non_null,
    // bstr,
    const_array,
    const_default,
    // const_format_args,
    const_index,
    const_option_ops,
    const_trait_impl,
    // core_intrinsics,
    // coroutine_clone,
    // coroutine_trait,
    // coroutines,
    cow_is_borrowed,
    // debug_closure_helpers,
    default_field_values,
    deref_patterns,
    derive_from,
    drop_guard,
    error_type_id,
    exact_div,
    // exact_size_is_empty,
    // exhaustive_patterns,
    // exit_status_error,
    f128,
    file_buffered,
    fmt_internals,
    // fn_traits,
    formatting_options,
    future_join,
    // gen_blocks,
    get_mut_unchecked,
    half_open_range_patterns_in_slices,
    // if_let_guard,
    impl_trait_in_assoc_type,
    int_format_into,
    io_const_error,
    // io_error_more,
    ip_as_octets,
    iter_array_chunks,
    iter_collect_into,
    // iter_from_coroutine,
    // iter_next_chunk,
    iter_partition_in_place,
    likely_unlikely,
    link_llvm_intrinsics,
    // map_try_insert,
    maybe_uninit_array_assume_init,
    maybe_uninit_fill,
    min_specialization,
    // negative_impls,
    // never_patterns,
    never_type,
    // new_range_api,
    os_str_slice,
    pattern,
    // pattern_types,
    // pattern_type_macro,
    // pattern_type_range_trait,
    postfix_match,
    ptr_as_ref_unchecked,
    // ptr_metadata,
    rustc_attrs,
    // simd_wasm64,
    // slice_ptr_get,
    slice_range,
    stmt_expr_attributes,
    str_internals,
    // string_deref_patterns,
    string_from_utf8_lossy_owned,
    // substr_range,
    // sync_unsafe_cell,
    // temporary_niche_types,
    // thin_box,
    // trait_alias,
    // transmutability,
    try_blocks,
    try_trait_v2,
    type_ascription,
    // unboxed_closures,
    unsafe_cell_access,
    // unsize,
    // unwrap_infallible,
    // yeet_expr,
)]

mod api;
mod libs;

#[tokio::main]
async fn main() -> std::io::Result<!> {
    use axum::{
        Router,
        extract::DefaultBodyLimit,
    };
    use hyper::server::conn;
    use hyper_util::rt::TokioIo;
    use tokio::net::UnixListener;

    use libs::request::RouterService;

    const SOCK: &str = "lean4oj.sock";

    libs::logger::init();

    libs::db::init_db().await;

    let mut app: Router = Router::new().nest("/api", api::all());

    app = app.layer(DefaultBodyLimit::disable());

    if let Err(err) = std::fs::remove_file(SOCK) && err.kind() != std::io::ErrorKind::NotFound {
        return Err(err);
    }

    let listener = UnixListener::bind(SOCK)?;
    // axum::serve(listener, app).await
    let mut http_builder = conn::http1::Builder::new();
    http_builder.auto_date_header(false);

    loop {
        let socket = match listener.accept().await {
            Ok((socket, _)) => socket,
            Err(e) => {
                tracing::warn!("server accept error: {e:?}");
                continue;
            }
        };

        tokio::spawn(
            http_builder
                .serve_connection(TokioIo::new(socket), RouterService(app.clone()))
                .with_upgrades(),
        );
    }
}
