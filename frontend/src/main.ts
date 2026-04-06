import init, { DaNode, initialize, FledgerState } from "@fledger/danu";
import { effect } from "@preact/signals-core";

const logsContainer = document.getElementById("logs") as HTMLDivElement;

let node: DaNode | null = null;

// Add log message to UI
function addLog(message: string, level: "info" | "error" | "success" = "info") {
  const logEntry = document.createElement("div");
  logEntry.className = `log-entry log-${level}`;
  const timestamp = new Date().toLocaleTimeString();
  logEntry.textContent = `[${timestamp}] ${message}`;
  logsContainer.appendChild(logEntry);
  logsContainer.scrollTop = logsContainer.scrollHeight;
}

// Initialize the application
async function initApp() {
  try {
    addLog("Initializing WASM module...", "info");
    await init();

    addLog("Setting up logging and panic hooks...", "info");
    initialize();

    addLog("Creating DaNode instance...", "info");
    node = await DaNode.from_default();
    node.set_status_div("danu-stats");
    addLog(`This is tab ${node.get_tab_id()}`);

    const rawState = await node.get_state();
    const state = await FledgerState.create(rawState);

    effect(() => {
      addLog(
        state.$signalConnected.value
          ? "Connected to Signal"
          : "Waiting for connection",
      );
    });
    effect(() => {
      addLog(`Nodes connected: ${state.$nodes_connected_dht.value.length}`);
    });
    effect(() => {
      addLog(`Nodes online: ${state.$nodes_online.value.length}`);
    });
    effect(() => {
      addLog(`Got new realm: ${state.$realm_ids.value}`);
    });

    addLog("Danu Browser initialized successfully", "success");
  } catch (error) {
    addLog(`Initialization error: ${error}`, "error");
    console.error("Initialization error:", error);
  }
}

// Initialize on page load
initApp();
