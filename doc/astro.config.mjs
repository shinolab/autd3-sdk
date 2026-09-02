// @ts-check
import { fileURLToPath } from "node:url";
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import starlightVersions from "starlight-versions";
import { sidebar } from "./src/sidebar.mjs";

// https://astro.build/config
export default defineConfig({
  site: "https://shinolab.github.io",
  base: "/autd3-sdk",
  redirects: {
    "/guide/appliance": "/autd3-sdk/getting-started/appliance/",
    "/en/guide/appliance": "/autd3-sdk/en/getting-started/appliance/",
    "/getting-started/setup/console": "/autd3-sdk/guide/console/",
    "/en/getting-started/setup/console": "/autd3-sdk/en/guide/console/",
  },
  vite: {
    resolve: {
      alias: {
        "@codes": fileURLToPath(new URL("./codes", import.meta.url)),
        "@components": fileURLToPath(new URL("./src/components", import.meta.url)),
        "@lib": fileURLToPath(new URL("./src/lib", import.meta.url)),
      },
    },
  },
  integrations: [
    starlight({
      title: "AUTD3 SDK",
      favicon: "/favicon.svg",
      expressiveCode: {
        defaultProps: {
          overridesByLang: {
            "bash,batch,bat,cmd,console,nu,nushell,powershell,ps,ps1,psd1,psm1,sh,shell,shellscript,shellsession,zsh":
              { frame: "none" },
          },
        },
        styleOverrides: {
          codeLineHeight: "1.3",
          codeFontFamily:
            'Menlo, Consolas, "Cascadia Mono", "DejaVu Sans Mono", "Noto Sans Mono", ui-monospace, monospace',
        },
      },
      components: {
        Header: "./src/components/Header.astro",
        Sidebar: "./src/components/Sidebar.astro",
        PageTitle: "./src/components/PageTitle.astro",
        Banner: "./src/components/Banner.astro",
        ThemeSelect: "./src/components/ThemeSelect.astro",
        Head: "./src/components/Head.astro",
      },
      locales: {
        root: { label: "日本語", lang: "ja" },
        en: { label: "English", lang: "en" },
      },
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/shinolab/autd3-sdk",
        },
      ],
      customCss: ["./src/styles/math.css", "./src/styles/sidebar.css", "./src/styles/nav.css"],
      head: [
        {
          tag: "link",
          attrs: { rel: "icon", href: "/autd3-sdk/favicon.ico", sizes: "32x32" },
        },
        {
          tag: "script",
          content:
            "try{if(localStorage.getItem('autd-sidebar-collapsed')==='1')document.documentElement.classList.add('autd-sidebar-collapsed')}catch(e){}",
        },
        {
          tag: "script",
          attrs: { src: "/autd3-sdk/autd-sidebar.js", defer: true },
        },
      ],
      plugins: [
        starlightVersions({
          current: { label: "git" },
          versions: [{ slug: "0.7.x" }, { slug: "0.6.x" }],
        }),
      ],
      sidebar,
    }),
  ],
});
