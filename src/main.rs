use anyhow::Result;
use wasmtime::*;
use wasmtime_wasi::{sync::WasiCtxBuilder, WasiCtx};

struct MyState {
    wasi: WasiCtx,
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut config = wasmtime::Config::new();
    config
        .async_support(true)
        .debug_info(false)
        // The behavior of fuel running out is defined on the Store
        .consume_fuel(true)
        .wasm_reference_types(true)
        .wasm_bulk_memory(true)
        .wasm_multi_value(true)
        .wasm_multi_memory(true)
        .cranelift_opt_level(wasmtime::OptLevel::SpeedAndSize)
        // Allocate resources on demand because we can't predict how many process will exist
        .allocation_strategy(wasmtime::InstanceAllocationStrategy::OnDemand)
        // Always use static memories
        .static_memory_forced(true);
    let engine = Engine::new(&config)?;
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::add_to_linker(&mut linker, |state: &mut MyState| &mut state.wasi)?;

    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .inherit_args()?
        .env("hello", "world")?
        .build();
    let mut store1 = Store::new(&engine, MyState { wasi });
    store1.out_of_fuel_async_yield(u64::MAX, 10000);

    // Instantiate our module with the imports we've created, and run it.
    let module = Module::from_file(&engine, "./src/test.wasm")?;
    let instance_pre = linker.instantiate_pre(&mut store1, &module)?;

    let instance = instance_pre.instantiate_async(&mut store1).await?;

    let handle = tokio::spawn(async move {
        instance
            .get_typed_func::<(), ()>(&mut store1, "_start")
            .unwrap()
            .call_async(&mut store1, ())
            .await
            .unwrap();
    });

    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .inherit_args()?
        .env("bar", "buz")?
        .build();
    let mut store2 = Store::new(&engine, MyState { wasi });
    store2.out_of_fuel_async_yield(u64::MAX, 10000);

    let instance2 = instance_pre.instantiate_async(&mut store2).await?;
    let handle2 = tokio::spawn(async move {
        instance2
            .get_typed_func::<(), ()>(&mut store2, "_start")
            .unwrap()
            .call_async(&mut store2, ())
            .await
            .unwrap();
    });

    match tokio::join!(handle, handle2) {
        (Ok(_), Ok(_)) => (),
        _ => (),
    };

    Ok(())
}
