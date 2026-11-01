import workspace from "../../Cargo.toml?raw";

const version = workspace.match(/^\[workspace\.package\][^[]*?^version = "(\d+)\.(\d+)\.\d+"/ms);

if (!version) throw new Error("The workspace Cargo.toml has no [workspace.package] version");

export const release = `${version[1]}.${version[2]}`;
export const tag = `v${release}`;
