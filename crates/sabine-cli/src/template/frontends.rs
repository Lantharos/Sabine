//! The web frameworks the Vite templates are made with.

macro_rules! file {
    ($path:literal) => {
        ($path, include_str!(concat!("frontends/", $path)))
    };
}

const TYPESCRIPT: (&str, &str) = ("typescript", "^7.0.2");
const TYPESCRIPT_6: (&str, &str) = ("typescript", "^6.0.3");
const VITE: (&str, &str) = ("vite", "^8.3.8");

pub(super) struct Frontend {
    pub template: &'static str,
    pub dependencies: &'static [(&'static str, &'static str)],
    pub dev_dependencies: &'static [(&'static str, &'static str)],
    pub check: &'static str,
    /// The import of the framework's Vite plugin, and the call that adds it.
    pub plugin: Option<(&'static str, &'static str)>,
    pub compiler_options: &'static [(&'static str, &'static str)],
    /// The page's markup: the framework mounts into `#app`.
    pub body: &'static str,
    pub entry: &'static str,
    /// Files below `ui/src`, by the path within the template's folder.
    pub files: &'static [(&'static str, &'static str)],
}

const MOUNT: &str = "<div id=\"app\"></div>";

pub(super) const FRONTENDS: [Frontend; 5] = [
    Frontend {
        template: "app",
        dependencies: &[],
        dev_dependencies: &[TYPESCRIPT, VITE],
        check: "tsc --noEmit",
        plugin: None,
        compiler_options: &[],
        body: "<main>\n      <h1>{name}</h1>\n      <p id=\"status\">Loading Sabine bridge…</p>\n      <button id=\"version\" type=\"button\">App version</button>\n    </main>",
        entry: "main.ts",
        files: &[file!("vanilla/main.ts")],
    },
    Frontend {
        template: "react",
        dependencies: &[("react", "^19.3.1"), ("react-dom", "^19.3.1")],
        dev_dependencies: &[
            ("@types/react", "^19.3.0"),
            ("@types/react-dom", "^19.3.0"),
            ("@vitejs/plugin-react", "^6.3.0"),
            TYPESCRIPT,
            VITE,
        ],
        check: "tsc --noEmit",
        plugin: Some(("import react from \"@vitejs/plugin-react\";", "react()")),
        compiler_options: &[("jsx", "react-jsx")],
        body: MOUNT,
        entry: "main.tsx",
        files: &[file!("react/main.tsx"), file!("react/App.tsx")],
    },
    Frontend {
        template: "svelte",
        dependencies: &[],
        dev_dependencies: &[
            ("@sveltejs/vite-plugin-svelte", "^7.3.1"),
            ("svelte", "^5.57.3"),
            ("svelte-check", "^4.7.8"),
            TYPESCRIPT_6,
            VITE,
        ],
        check: "svelte-check --tsconfig ./tsconfig.json",
        plugin: Some((
            "import { svelte } from \"@sveltejs/vite-plugin-svelte\";",
            "svelte()",
        )),
        compiler_options: &[],
        body: MOUNT,
        entry: "main.ts",
        files: &[file!("svelte/main.ts"), file!("svelte/App.svelte")],
    },
    Frontend {
        template: "vue",
        dependencies: &[("vue", "^3.5.45")],
        dev_dependencies: &[
            ("@vitejs/plugin-vue", "^6.0.11"),
            TYPESCRIPT_6,
            VITE,
            ("vue-tsc", "^3.3.14"),
        ],
        check: "vue-tsc --noEmit",
        plugin: Some(("import vue from \"@vitejs/plugin-vue\";", "vue()")),
        compiler_options: &[],
        body: MOUNT,
        entry: "main.ts",
        files: &[file!("vue/main.ts"), file!("vue/App.vue")],
    },
    Frontend {
        template: "solid",
        dependencies: &[("solid-js", "^1.9.19")],
        dev_dependencies: &[TYPESCRIPT, VITE, ("vite-plugin-solid", "^2.11.16")],
        check: "tsc --noEmit",
        plugin: Some(("import solid from \"vite-plugin-solid\";", "solid()")),
        compiler_options: &[("jsx", "preserve"), ("jsxImportSource", "solid-js")],
        body: MOUNT,
        entry: "main.tsx",
        files: &[file!("solid/main.tsx"), file!("solid/App.tsx")],
    },
];
