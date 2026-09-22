use std::path::PathBuf;

fn main() -> Result<(), String> {
    let mut arguments = std::env::args_os().skip(1);
    let host = arguments.next().map(PathBuf::from);
    let runtime = arguments.next().map(PathBuf::from);
    let (Some(host), Some(runtime), None) = (host, runtime, arguments.next()) else {
        return Err("usage: runtime_probe <host executable> <CEF runtime directory>".into());
    };
    sabine_host::smoke_test_runtime(&host, &runtime)?;
    println!(
        "{}",
        sabine_host::prepare_host_execution(&host, &runtime)?.display()
    );
    Ok(())
}
