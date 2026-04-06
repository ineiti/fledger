import { describe, it, expect, vi } from "vitest";
import { effect } from "@preact/signals-core";
import { FledgerState } from "./state.js";
import type { StateUpdateMsg } from "./state.js";

// Helper to create a controllable ReadableStream
function makeStream<T>(): {
  stream: ReadableStream<T>;
  push: (v: T) => void;
  close: () => void;
} {
  let ctrl!: ReadableStreamDefaultController<T>;
  const stream = new ReadableStream<T>({
    start(c) {
      ctrl = c;
    },
  });
  return {
    stream,
    push: (v) => ctrl.enqueue(v),
    close: () => ctrl.close(),
  };
}

// Minimal State mock — only fields we test against
function makeState(overrides: Partial<StateUpdateMsg["state"]> = {}): StateUpdateMsg["state"] {
  return {
    config: null,
    node_info: { name: "test", id: "node-0" } as any,
    realm_ids: [],
    nodes_connected_dht: [],
    nodes_online: [],
    dht_storage_stats: { stored: 0, requested: 0 } as any,
    flos: {} as any,
    is_leader: null,
    tab_list: [],
    ...overrides,
  };
}


describe("FledgerState", () => {
  it("signalConnected starts false", async () => {
    const s = makeStream<StateUpdateMsg>();
    const state = await FledgerState.create(s.stream);
    expect(state.signalConnected).toBe(false);
  });

  it("signalConnected becomes true after ConnectSignal", async () => {
    const s = makeStream<StateUpdateMsg>();
    const state = await FledgerState.create(s.stream);
    s.push({ update: "ConnectSignal", state: makeState() });
    // Allow microtask to process
    await new Promise((r) => setTimeout(r, 0));
    expect(state.signalConnected).toBe(true);
  });

  it("nodes_connected_dht starts empty", async () => {
    const s = makeStream<StateUpdateMsg>();
    const state = await FledgerState.create(s.stream);
    expect(state.nodes_connected_dht).toEqual([]);
  });

  it("nodes_connected_dht updates on ConnectedNodes", async () => {
    const s = makeStream<StateUpdateMsg>();
    const state = await FledgerState.create(s.stream);
    s.push({
      update: "ConnectedNodes",
      state: makeState({ nodes_connected_dht: ["node-1", "node-2"] as any }),
    });
    await new Promise((r) => setTimeout(r, 0));
    expect(state.nodes_connected_dht).toEqual(["node-1", "node-2"]);
  });

  it("nodes_connected_dht clears on DisconnectNodes", async () => {
    const s = makeStream<StateUpdateMsg>();
    const state = await FledgerState.create(s.stream);
    s.push({
      update: "ConnectedNodes",
      state: makeState({ nodes_connected_dht: ["node-1"] as any }),
    });
    await new Promise((r) => setTimeout(r, 0));
    s.push({
      update: "DisconnectNodes",
      state: makeState({ nodes_connected_dht: [] }),
    });
    await new Promise((r) => setTimeout(r, 0));
    expect(state.nodes_connected_dht).toEqual([]);
  });

  it("nodes_online updates on AvailableNodes", async () => {
    const s = makeStream<StateUpdateMsg>();
    const state = await FledgerState.create(s.stream);
    const nodeInfo = { name: "peer", id: "node-x" } as any;
    s.push({
      update: { AvailableNodes: [nodeInfo] },
      state: makeState(),
    });
    await new Promise((r) => setTimeout(r, 0));
    expect(state.nodes_online).toEqual([nodeInfo]);
  });

  it("realm_ids updates on RealmAvailable", async () => {
    const s = makeStream<StateUpdateMsg>();
    const state = await FledgerState.create(s.stream);
    const rid = "realm-abc" as any;
    s.push({
      update: { RealmAvailable: [rid] },
      state: makeState(),
    });
    await new Promise((r) => setTimeout(r, 0));
    expect(state.realm_ids).toEqual([rid]);
  });

  it("tab_list updates on TabList", async () => {
    const s = makeStream<StateUpdateMsg>();
    const state = await FledgerState.create(s.stream);
    s.push({
      update: { TabList: ["tab-1", "tab-2"] as any },
      state: makeState(),
    });
    await new Promise((r) => setTimeout(r, 0));
    expect(state.tab_list).toEqual(["tab-1", "tab-2"]);
  });

  it("is_leader updates on NewLeader", async () => {
    const s = makeStream<StateUpdateMsg>();
    const state = await FledgerState.create(s.stream);
    s.push({
      update: { NewLeader: "tab-1" as any },
      state: makeState({ is_leader: true }),
    });
    await new Promise((r) => setTimeout(r, 0));
    expect(state.is_leader).toBe(true);
  });

  it("dht_storage_stats updates on DHTStorageStats", async () => {
    const s = makeStream<StateUpdateMsg>();
    const state = await FledgerState.create(s.stream);
    const stats = { stored: 42, requested: 7 } as any;
    s.push({
      update: "DHTStorageStats",
      state: makeState({ dht_storage_stats: stats }),
    });
    await new Promise((r) => setTimeout(r, 0));
    expect(state.dht_storage_stats).toEqual(stats);
  });

  it("properties return plain values, not Signal objects", async () => {
    const s = makeStream<StateUpdateMsg>();
    const state = await FledgerState.create(s.stream);
    expect(typeof state.signalConnected).toBe("boolean");
    expect(Array.isArray(state.nodes_connected_dht)).toBe(true);
  });

  it("effect() re-runs when signalConnected changes", async () => {
    const s = makeStream<StateUpdateMsg>();
    const state = await FledgerState.create(s.stream);

    const calls: boolean[] = [];
    const dispose = effect(() => {
      calls.push(state.signalConnected);
    });

    // effect runs once immediately
    expect(calls).toEqual([false]);

    s.push({ update: "ConnectSignal", state: makeState() });
    await new Promise((r) => setTimeout(r, 0));

    expect(calls).toEqual([false, true]);
    dispose();
  });

  it("effect() re-runs when nodes_connected_dht changes", async () => {
    const s = makeStream<StateUpdateMsg>();
    const state = await FledgerState.create(s.stream);

    const lengths: number[] = [];
    const dispose = effect(() => {
      lengths.push(state.nodes_connected_dht.length);
    });

    expect(lengths).toEqual([0]);

    s.push({
      update: "ConnectedNodes",
      state: makeState({ nodes_connected_dht: ["n1", "n2"] as any }),
    });
    await new Promise((r) => setTimeout(r, 0));

    expect(lengths).toEqual([0, 2]);
    dispose();
  });
});
