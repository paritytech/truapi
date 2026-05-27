import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import "./index.css";
import App from "./App";

// Mount path, e.g. "/explorer" or "/truapi/explorer", derived from Vite's base.
const basename = import.meta.env.BASE_URL.replace(/\/$/, "");

// Replay a `?p=/<path>` query produced by the GH Pages 404 shim before the
// router mounts.
const params = new URLSearchParams(window.location.search);
const replay = params.get("p");
if (replay) {
  const rest = new URLSearchParams(window.location.search);
  rest.delete("p");
  const tail = rest.toString();
  const next =
    basename + replay + (tail ? `?${tail}` : "") + window.location.hash;
  window.history.replaceState(null, "", next);
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BrowserRouter basename={basename}>
      <App />
    </BrowserRouter>
  </StrictMode>,
);
