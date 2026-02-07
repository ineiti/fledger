import init, { DaNode, initialize, NodeStatus, RealmID } from "@danu/danu";

// DOM elements
const connectionStatus = document.getElementById(
  "connection-status",
) as HTMLSpanElement;
const onlineNodes = document.getElementById("online-nodes") as HTMLSpanElement;
const dhtConnections = document.getElementById(
  "dht-connections",
) as HTMLSpanElement;
const messagesSent = document.getElementById(
  "messages-sent",
) as HTMLSpanElement;
const messagesReceived = document.getElementById(
  "messages-received",
) as HTMLSpanElement;
const uptime = document.getElementById("uptime") as HTMLSpanElement;
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
    node.set_event_listener(new_event);

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
      connectionStatus.textContent = "Connected";
      connectionStatus.className = `status-value connected`;
      break;
    case NodeStatus.DisconnectSignal:
      connectionStatus.textContent = "Disconnected";
      connectionStatus.className = `status-value disconnected`;
      break;
    case NodeStatus.ConnectedNodes:
      addLog(`Connected to ${args} nodes`);
      dhtConnections.textContent = args.toString();
      break;
    case NodeStatus.AvailableNodes:
      addLog(`${args} nodes available`);
      onlineNodes.textContent = args.toString();
      break;
    case NodeStatus.RealmAvailable:
      addLog(`${args.length} realms available`);
      realms.splice(0);
      realms.push(...args);
  }
}

// Initialize on page load
initApp();
