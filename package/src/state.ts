import { signal } from "@preact/signals-core";
import type { Signal } from "@preact/signals-core";
import type { State, FloID, RealmID, TabID } from "../pkg/danu.js";

// Serde externally-tagged enum representation of StateUpdate
export type StateUpdateTS =
  | "ConnectSignal"
  | "ConnectedNodes"
  | "DisconnectNodes"
  | "DHTStorageStats"
  | { AvailableNodes: State["nodes_online"] }
  | { RealmAvailable: State["realm_ids"] }
  | { ReceivedFlo: FloID }
  | { NewLeader: TabID }
  | { TabList: State["tab_list"] }
  | { SystemRealm: RealmID };

export interface StateUpdateMsg {
  update: StateUpdateTS;
  state: State;
}

export interface IStateObserver {
  listen_updates(): Promise<ReadableStream<StateUpdateMsg>>;
}

export class FledgerState {
  // Public signals — subscribe with effect() from @preact/signals-core
  readonly $signalConnected: Signal<boolean> = signal(false);
  readonly $nodes_connected_dht: Signal<State["nodes_connected_dht"]> = signal([]);
  readonly $nodes_online: Signal<State["nodes_online"]> = signal([]);
  readonly $realm_ids: Signal<State["realm_ids"]> = signal([]);
  readonly $dht_storage_stats: Signal<State["dht_storage_stats"] | null> = signal(null);
  readonly $is_leader: Signal<boolean | null> = signal(null);
  readonly $tab_list: Signal<State["tab_list"]> = signal([]);

  // Convenience getters for plain values
  get signalConnected() { return this.$signalConnected.value; }
  get nodes_connected_dht() { return this.$nodes_connected_dht.value; }
  get nodes_online() { return this.$nodes_online.value; }
  get realm_ids() { return this.$realm_ids.value; }
  get dht_storage_stats() { return this.$dht_storage_stats.value; }
  get is_leader() { return this.$is_leader.value; }
  get tab_list() { return this.$tab_list.value; }

  private constructor() {}

  static async create(observer: IStateObserver): Promise<FledgerState> {
    const instance = new FledgerState();
    const stream = await observer.listen_updates();
    instance._readLoop(stream);
    return instance;
  }

  private _readLoop(stream: ReadableStream<StateUpdateMsg>): void {
    (async () => {
      const reader = stream.getReader();
      while (true) {
        const { value, done } = await reader.read();
        if (done) break;
        this._applyUpdate(value);
      }
    })();
  }

  private _applyUpdate({ update, state }: StateUpdateMsg): void {
    if (update === "ConnectSignal") {
      this.$signalConnected.value = true;
    } else if (update === "DisconnectNodes") {
      this.$signalConnected.value = false;
      this.$nodes_connected_dht.value = state.nodes_connected_dht;
    } else if (update === "ConnectedNodes") {
      this.$nodes_connected_dht.value = state.nodes_connected_dht;
    } else if (update === "DHTStorageStats") {
      this.$dht_storage_stats.value = state.dht_storage_stats;
    } else if (typeof update === "object") {
      if ("AvailableNodes" in update) {
        this.$nodes_online.value = update.AvailableNodes;
      } else if ("RealmAvailable" in update) {
        this.$realm_ids.value = update.RealmAvailable;
      } else if ("NewLeader" in update) {
        this.$is_leader.value = state.is_leader ?? null;
      } else if ("TabList" in update) {
        this.$tab_list.value = update.TabList;
      }
      // ReceivedFlo and SystemRealm are intentionally not handled here:
      // ReceivedFlo is consumed by RealmObserver streams.
      // SystemRealm is informational and not reflected in FledgerState.
    }
  }
}
