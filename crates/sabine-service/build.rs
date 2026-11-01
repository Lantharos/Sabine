fn main() {
    println!("cargo:rerun-if-changed=icons/sabine.rc");
    println!("cargo:rerun-if-changed=icons/sabine.ico");
    println!("cargo:rerun-if-env-changed=SABINE_ENV");
    println!("cargo:rustc-check-cfg=cfg(sabine_production_build)");
    if std::env::var("SABINE_ENV").as_deref() == Ok("production") {
        println!("cargo:rustc-cfg=sabine_production_build");
    }
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
