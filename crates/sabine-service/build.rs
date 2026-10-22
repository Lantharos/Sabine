fn main() {
    println!("cargo:rerun-if-changed=icons/sabine.rc");
    println!("cargo:rerun-if-changed=icons/sabine.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    embed_resource::compile_for(
        "icons/sabine.rc",
        ["sabine-service", "sabine-service-daemon"],
        embed_resource::ParamsIncludeDirs(["icons"]),
    )
    .manifest_required()
    .unwrap();
}
