import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import "./styles.css";

const label = getCurrentWindow().label;

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App windowLabel={label} />
  </StrictMode>,
);
