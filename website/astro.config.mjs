import { defineConfig, fontProviders } from "astro/config";
import starlight from "@astrojs/starlight";
import tailwindcss from "@tailwindcss/vite";
import starlightLinksValidator from "starlight-links-validator";

const group = (label, directory) => ({ label, items: [{ autogenerate: { directory: `docs/${directory}` } }] });

export default defineConfig({
  site: "https://sabine.rs",
  devToolbar: { enabled: false },
  fonts: [
    {
      provider: fontProviders.fontsource(),
      name: "Hanken Grotesk",
      cssVariable: "--font-hanken",
      weights: ["100 900"],
      styles: ["normal"],
      subsets: ["latin"],
      display: "block",
      fallbacks: ["ui-sans-serif", "system-ui", "sans-serif"],
    },
    {
      provider: fontProviders.fontsource(),
      name: "Geist Mono",
      cssVariable: "--font-geist-mono",
      weights: ["100 900"],
      styles: ["normal"],
      subsets: ["latin"],
      display: "block",
      fallbacks: ["ui-monospace", "monospace"],
    },
  ],
  redirects: { "/docs": "/docs/start/introduction/" },
  integrations: [
    starlight({
      title: "Sabine",
      description: "Build native desktop apps in Rust with a web interface, on Linux, Windows and macOS.",
      favicon: "/favicon.svg",
      social: [{ icon: "github", label: "GitHub", href: "https://github.com/Lantharos/Sabine" }],
      editLink: { baseUrl: "https://github.com/Lantharos/Sabine/edit/main/website/" },
      customCss: ["./src/styles/docs.css"],
      plugins: [starlightLinksValidator({ errorOnLocalLinks: false })],
      lastUpdated: true,
      credits: false,
      tableOfContents: { minHeadingLevel: 2, maxHeadingLevel: 3 },
      expressiveCode: {
        themes: ["github-dark-default", "github-light"],
        styleOverrides: {
          borderRadius: "0.6rem",
          borderColor: "var(--sl-color-hairline)",
          codeFontFamily: "var(--font-mono)",
          codeBackground: "var(--sabine-code)",
          uiFontFamily: "var(--font-sans)",
          frames: {
            shadowColor: "transparent",
            editorTabBarBackground: "var(--sl-color-gray-6)",
            terminalTitlebarBackground: "var(--sl-color-gray-6)",
            terminalBackground: "var(--sabine-code)",
          },
        },
      },
      components: {
        Head: "./src/components/docs/Head.astro",
        SiteTitle: "./src/components/docs/SiteTitle.astro",
      },
      sidebar: [
        group("Start here", "start"),
        group("Windows", "windows"),
        group("The bridge", "bridge"),
        group("Web content", "web"),
        group("Desktop integration", "desktop"),
        group("Shipping your app", "distribution"),
        group("How Sabine works", "concepts"),
        group("Reference", "reference"),
        group("Contributing", "contributing"),
      ],
    }),
  ],
  vite: { plugins: [tailwindcss()] },
});
