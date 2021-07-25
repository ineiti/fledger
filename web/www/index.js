import { FledgerWeb } from "fledger-web";

const fw = FledgerWeb.new();
console.log("started fledger web");

setInterval(() => {
    const state = fw.tick();
    update_html(state);
}, 1000);

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
        document.getElementById("table_stats").innerHTML = stats_table;
    }
    document.getElementById("node_info").innerHTML = state.get_node_name();
}