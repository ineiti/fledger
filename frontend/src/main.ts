import init, { DaNode, initialize, NodeStatus, RealmID } from "@danu/danu";

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
    node.set_event_listener(new_event);
    addLog(`This is tab ${node.get_tab_id()}`);

    addLog("Danu Browser initialized successfully", "success");
  } catch (error) {
    addLog(`Initialization error: ${error}`, "error");
    console.error("Initialization error:", error);
  }
}

const realms: RealmID[] = [];

async function new_event(status: NodeStatus, args: any) {
  switch (status) {
    case NodeStatus.ConnectSignal:
      addLog("Connected to Signal");
      break;
    case NodeStatus.DisconnectSignal:
      addLog("Disconnected from Signal");
      break;
    case NodeStatus.ConnectedNodes:
      addLog(`Connected to ${args} nodes`);
      break;
    case NodeStatus.AvailableNodes:
      addLog(`${args} nodes available`);
      break;
    case NodeStatus.RealmAvailable:
      addLog(`${args.length} realms available`);
      break;
    case NodeStatus.IsLeader:
      addLog(`Got elected as leader`);
      break;
    case NodeStatus.TabsCount:
      addLog(`${args} tabs available`);
      break;
  }
}

// Initialize on page load
initApp();
