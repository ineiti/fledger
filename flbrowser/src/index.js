import { FledgerWeb } from "fledger-web";

const fw = FledgerWeb.new();

setInterval(() => {
    const state = fw.tick();
    if (state !== undefined){
        update_html(state);
    }
}, 1000);

function sh(tag, str) {
    document.getElementById(tag).innerHTML = str;
}

function update_html(state) {
    let stats_table = state.get_node_table();
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
    sh("messages", state.get_msgs())
    sh("nodes_known", state.nodes_known);
    sh("nodes_online", state.nodes_online);
    sh("nodes_connected", state.nodes_connected);
}

document.getElementById("send_msg").addEventListener("click", event => {
    let msg = document.getElementById("your_message").value;
    if (msg == ""){
        alert("Please enter some message");
        return;
    }
    fw.send_msg(msg);
});