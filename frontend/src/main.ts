import init, { DaNode, initialize, NodeStatus } from "@danu/danu";

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

    addLog("Creating FlNode instance...", "info");
    node = await DaNode.from_default();

    // Set up event listener to receive events from the package
    node.set_event_listener(new_event);
    /*
      dhtConnections.textContent = status.dht_connections.toString();

      const stats = node.get_stats();
      messagesSent.textContent = stats.messages_sent.toString();
      messagesReceived.textContent = stats.messages_received.toString();
      uptime.textContent = `${stats.uptime_seconds}s`;
 */

    addLog("Fledger Browser initialized successfully", "success");
  } catch (error) {
    addLog(`Initialization error: ${error}`, "error");
    console.error("Initialization error:", error);
  }
}

function new_event(status: NodeStatus, args: any) {
  console.log(`Handling Event: ${status} - args: ${args}`);
  switch (status) {
    case NodeStatus.ConnectSignal:
      connectionStatus.textContent = "Connected";
      connectionStatus.className = `status-value connected`;
      break;
    case NodeStatus.DisconnectSignal:
      connectionStatus.textContent = "Disconnected";
      connectionStatus.className = `status-value disconnected`;
      break;
    case NodeStatus.ConnectedNodes:
      dhtConnections.textContent = args.toString();
      break;
    case NodeStatus.AvailableNodes:
      onlineNodes.textContent = args.toString();
      break;
    case NodeStatus.RealmAvailable:
      console.log("Got realms:");
      for (const realm of args) {
        console.log(realm);
      }
  }
}

// Initialize on page load
initApp();
