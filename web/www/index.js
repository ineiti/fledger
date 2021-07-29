import { FledgerWeb } from "fledger-web";

const fw = FledgerWeb.new();
console.log("started fledger web");

setInterval(() => {
    const state = fw.tick();
    update_html(state);
}, 1000);

function sh(tag, str) {
    document.getElementById(tag).innerHTML = str;
}

function update_html(state) {
    let stats_table = state.get_stats_table();
    if (stats_table === "") {
        return;
    }
    let el_fetching = document.getElementById("fetching");
    let el_table_stats = document.getElementById("table_stats");
    el_table_stats.classList.remove("hidden");
    el_fetching.classList.remove("hidden");
    if (stats_table == "") {
        el_table_stats.classList.add("hidden");
    } else {
        el_fetching.classList.add("hidden");
        sh("node_stats", stats_table);
    }
    sh("node_info", state.get_node_name());
    sh("version", state.get_version());
    sh("nodes_online", state.get_nodes_online());
    sh("msgs_system", state.get_msgs_system());
    sh("msgs_local", state.get_msgs_local());
    sh("mana", state.get_mana());
}