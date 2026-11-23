import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";
import { perf } from "./lib/perf";

mount(App, { target: document.getElementById("app")! });

// The instrumentation behind `window.keelPerf.report()`: streamed-event rates,
// markdown sample cost, and the frames the browser could not deliver.
perf.install();
