// @ts-check
import { fileURLToPath } from "node:url";
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import starlightVersions from "starlight-versions";

// https://astro.build/config
export default defineConfig({
  site: "https://shinolab.github.io",
  base: "/autd3-sdk",
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
      expressiveCode: {
        styleOverrides: { codeLineHeight: "1.3" },
      },
      components: {
        Sidebar: "./src/components/Sidebar.astro",
        PageTitle: "./src/components/PageTitle.astro",
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
      customCss: ["./src/styles/math.css", "./src/styles/sidebar.css"],
      head: [
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
          versions: [{ slug: "0.1.x" }],
        }),
      ],
      sidebar: [
        { label: "はじめに", translations: { en: "Introduction" }, link: "/" },
        {
          label: "Getting Started",
          items: [
            {
              label: "セットアップ",
              translations: { en: "Setup" },
              items: [
                { label: "概要", translations: { en: "Overview" }, link: "/getting-started/setup/" },
                { label: "ハードウェア", translations: { en: "Hardware" }, link: "/getting-started/setup/hardware/" },
                { label: "ファームウェア", translations: { en: "Firmware" }, link: "/getting-started/setup/firmware/" },
                { label: "ソフトウェア", translations: { en: "Software" }, link: "/getting-started/setup/software/" },
              ],
            },
            {
              label: "TwinCAT のインストール",
              translations: { en: "Installing TwinCAT" },
              link: "/getting-started/twincat/",
            },
            {
              label: "チュートリアル",
              translations: { en: "Tutorial" },
              items: [{ autogenerate: { directory: "getting-started/tutorial" } }],
            },
          ],
        },
        {
          label: "ガイド",
          translations: { en: "Guide" },
          items: [
            { autogenerate: { directory: "guide" } },
            {
              label: "ハードウェア",
              translations: { en: "Hardware" },
              items: [{ autogenerate: { directory: "hardware" } }],
            },
          ],
        },
        {
          label: "API リファレンス",
          translations: { en: "API Reference" },
          items: [
            { label: "基礎", items: [{ autogenerate: { directory: "api/basics" } }] },
            { label: "コマンド", items: [{ autogenerate: { directory: "api/command" } }] },
            {
              label: "計算ヘルパ",
              items: [{ autogenerate: { directory: "api/compute" } }],
            },
            { label: "Link", items: [{ autogenerate: { directory: "api/link" } }] },
          ],
        },
        {
          label: "その他",
          translations: { en: "Misc" },
          items: [
            { autogenerate: { directory: "misc" } },
            {
              label: "リリースノート",
              translations: { en: "Release Notes" },
              link: "https://github.com/shinolab/autd3-sdk/blob/main/CHANGELOG.md",
              attrs: { target: "_blank", rel: "noopener" },
            },
          ],
        },
      ],
    }),
  ],
});
