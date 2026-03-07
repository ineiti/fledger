import init, {
  DaNode,
  initialize,
  RealmID,
  State,
  StateUpdate,
} from "@fledger/danu";

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
    // node.set_event_listener(new_event);
    addLog(`This is tab ${node.get_tab_id()}`);

    addLog("Danu Browser initialized successfully", "success");
  } catch (error) {
    addLog(`Initialization error: ${error}`, "error");
    console.error("Initialization error:", error);
  }
}

let realms: RealmID[] = [];

async function new_event(status: StateUpdate, state: State) {
  switch (status) {
    case StateUpdate.ConnectSignal:
      addLog("Connected to Signal");
      break;
    case StateUpdate.ConnectedNodes:
      addLog(`Connected to ${state.nodes_connected_dht.length} nodes`);
      break;
    case StateUpdate.AvailableNodes:
      addLog(`${state.nodes_online.length} nodes available`);
      break;
    case StateUpdate.RealmAvailable:
      realms = state.realm_ids;
      addLog(`${state.realm_ids.length} realms available`);
      break;
    case StateUpdate.NewLeader:
      let role = "Searching";
      if (state.is_leader !== null) {
        role = state.is_leader ? "leader-tab" : "follower-tab";
      }
      addLog(`New leader got elected - ${role}`);
      break;
    case StateUpdate.TabList:
      addLog(`${state.tab_list.length} tabs available`);
      break;
  }
}

// import { signal, effect } from "@preact/signals-core";
// let dn = DaNode.from_default();
// let realm = dn.get_realm();
// realm.set_update_callback((flo_realm) => console.log(flo_realm));
// let flo_page = realm.get_flo_page_path("/");
// flo_page.set_update_callback((fp) => console.log(fp));
// flo_page.update((fp) => {
//   fp.set_index("<html><h1>New");
// });

// Initialize on page load
initApp();
