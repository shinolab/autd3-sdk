import { mathjax } from "@mathjax/src/js/mathjax.js";
import { TeX } from "@mathjax/src/js/input/tex.js";
import { SVG } from "@mathjax/src/js/output/svg.js";
import { liteAdaptor } from "@mathjax/src/js/adaptors/liteAdaptor.js";
import { RegisterHTMLHandler } from "@mathjax/src/js/handlers/html.js";
import { MathJaxNewcmFont } from "@mathjax/mathjax-newcm-font/js/svg.js";

import "@mathjax/src/js/input/tex/base/BaseConfiguration.js";
import "@mathjax/src/js/input/tex/action/ActionConfiguration.js";
import "@mathjax/src/js/input/tex/ams/AmsConfiguration.js";
import "@mathjax/src/js/input/tex/amscd/AmsCdConfiguration.js";
import "@mathjax/src/js/input/tex/bbox/BboxConfiguration.js";
import "@mathjax/src/js/input/tex/boldsymbol/BoldsymbolConfiguration.js";
import "@mathjax/src/js/input/tex/braket/BraketConfiguration.js";
import "@mathjax/src/js/input/tex/bussproofs/BussproofsConfiguration.js";
import "@mathjax/src/js/input/tex/cancel/CancelConfiguration.js";
import "@mathjax/src/js/input/tex/cases/CasesConfiguration.js";
import "@mathjax/src/js/input/tex/centernot/CenternotConfiguration.js";
import "@mathjax/src/js/input/tex/color/ColorConfiguration.js";
import "@mathjax/src/js/input/tex/colortbl/ColortblConfiguration.js";
import "@mathjax/src/js/input/tex/configmacros/ConfigMacrosConfiguration.js";
import "@mathjax/src/js/input/tex/empheq/EmpheqConfiguration.js";
import "@mathjax/src/js/input/tex/enclose/EncloseConfiguration.js";
import "@mathjax/src/js/input/tex/extpfeil/ExtpfeilConfiguration.js";
import "@mathjax/src/js/input/tex/gensymb/GensymbConfiguration.js";
import "@mathjax/src/js/input/tex/html/HtmlConfiguration.js";
import "@mathjax/src/js/input/tex/mathtools/MathtoolsConfiguration.js";
import "@mathjax/src/js/input/tex/mhchem/MhchemConfiguration.js";
import "@mathjax/src/js/input/tex/newcommand/NewcommandConfiguration.js";
import "@mathjax/src/js/input/tex/noerrors/NoErrorsConfiguration.js";
import "@mathjax/src/js/input/tex/noundefined/NoUndefinedConfiguration.js";
import "@mathjax/src/js/input/tex/upgreek/UpgreekConfiguration.js";
import "@mathjax/src/js/input/tex/unicode/UnicodeConfiguration.js";
import "@mathjax/src/js/input/tex/verb/VerbConfiguration.js";
import "@mathjax/src/js/input/tex/tagformat/TagFormatConfiguration.js";
import "@mathjax/src/js/input/tex/textcomp/TextcompConfiguration.js";
import "@mathjax/src/js/input/tex/textmacros/TextMacrosConfiguration.js";

const packages = [
  "base",
  "action",
  "ams",
  "amscd",
  "bbox",
  "boldsymbol",
  "braket",
  "bussproofs",
  "cancel",
  "cases",
  "centernot",
  "color",
  "colortbl",
  "empheq",
  "enclose",
  "extpfeil",
  "gensymb",
  "html",
  "mathtools",
  "mhchem",
  "newcommand",
  "noerrors",
  "noundefined",
  "upgreek",
  "unicode",
  "verb",
  "configmacros",
  "tagformat",
  "textcomp",
  "textmacros",
];

const macros: Record<string, string | [string, number]> = {
  SI: ["{#1\\,\\mathrm{#2}}", 2],
  rme: "\\mathrm{e}",
  im: "\\mathrm{i}",
  bzero: "\\boldsymbol{0}",
  bone: "\\boldsymbol{1}",
  ba: "\\boldsymbol{a}",
  boldf: "\\boldsymbol{f}",
  bk: "\\boldsymbol{k}",
  bp: "\\boldsymbol{p}",
  bq: "\\boldsymbol{q}",
  br: "\\boldsymbol{r}",
  bu: "\\boldsymbol{u}",
  bv: "\\boldsymbol{v}",
  bx: "\\boldsymbol{x}",
  by: "\\boldsymbol{y}",
  bz: "\\boldsymbol{z}",
  bS: "\\boldsymbol{S}",
  bgamma: "\\boldsymbol{\\gamma}",
  btheta: "\\boldsymbol{\\theta}",
  bphi: "\\boldsymbol{\\phi}",
  bpsi: "\\boldsymbol{\\psi}",
  bxi: "\\boldsymbol{\\xi}",
  trans: "^\\mathsf{T}",
  hermite: "^\\dagger",
  ufreq: "\\SI{40}{kHz}",
  clkf: "\\SI{10.24}{MHz}",
  sinc: "\\mathrm{sinc}",
  diag: "\\mathrm{diag}",
  diff: ["\\frac{\\mathrm{d}#1}{\\mathrm{d}#2}", 2],
  pdiff: ["\\frac{\\partial#1}{\\partial#2}", 2],
  pdiffs: ["\\frac{\\partial^2#1}{\\partial#2^2}", 2],
};

const adaptor = liteAdaptor();
RegisterHTMLHandler(adaptor);

const tex = new TeX({ packages, macros });
const svg = new SVG({
  fontCache: "none",
  fontData: MathJaxNewcmFont,
  linebreaks: { inline: false },
});
const doc = mathjax.document("", { InputJax: tex, OutputJax: svg });

mathjax.asyncLoad = (name: string) => import(/* @vite-ignore */ name);
await (svg as unknown as { font: { loadDynamicFiles(): Promise<unknown> } }).font.loadDynamicFiles();

export function renderTeX(source: string, display: boolean): string {
  const node = doc.convert(source, { display });
  return adaptor.outerHTML(node);
}
