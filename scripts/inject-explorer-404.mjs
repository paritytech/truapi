// Inject an explorer SPA-fallback shim into the site-root 404.html.
//
// The combined GitHub Pages site lives under /truapi/, and GitHub Pages serves
// only the site-root 404.html for every missing path, including the explorer's
// client-side routes (e.g. /truapi/explorer/v/main/method/foo). Without this,
// refreshing an explorer deep link falls through to the playground's Next.js
// not-found page. The shim redirects /explorer/* paths into the explorer index
// via the `?p=` query its bootstrap replays; all other paths keep the
// playground 404 untouched.
import { readFileSync, writeFileSync } from "node:fs";

const file = process.argv[2];
if (!file) {
  console.error("usage: inject-explorer-404.mjs <path-to-404.html>");
  process.exit(1);
}

const shim =
  "<script>(function(){" +
  'var m="/explorer/",p=location.pathname,i=p.indexOf(m);' +
  "if(i<0)return;" +
  "var b=p.slice(0,i+m.length),r=p.slice(i+m.length);" +
  "if(!r)return;" +
  "var s=location.search,h=location.hash;" +
  'location.replace(b+"?p=/"+r+(s?"&"+s.slice(1):"")+h);' +
  "})();</script>";

const html = readFileSync(file, "utf8");
if (!html.includes("<head>")) {
  console.error(`no <head> found in ${file}; cannot inject SPA fallback`);
  process.exit(1);
}
writeFileSync(file, html.replace("<head>", "<head>" + shim));
console.log(`injected explorer SPA fallback into ${file}`);
