# SHAREIT

Wanna create a little webpage? But don't have a provider?
Or you don't want to rely on one of the free providers, as they have too much ads?

Share your connection, harddisk-space, and CPU, and if others do the same, your page can be up 24/7!

For any minute you share another webpage, your webpage will be hosted on another node, too!

## Fledger plugin

This is the first usable extension of the networking layer of fledger.
It does not use any blockchain yet, but relies on a set of trusted servers.

### Trusted servers

- v1: the trusted servers are statically defined
- v2: the most reliable nodes are promoted as the trusted servers

## How it works

1. You connect to the fledger network
2. Your node starts downloading other people's webpage and hosts them, providing your node with Mana by doing so
3. You write your one-file webpage and publish it to the network, providing it with some of the Mana you got
4. As soon as your node disconnects, your webpage will be served by other nodes
5. Everytime your webpage is served, some Mana is used. When it drops to 0, your page will be deleted

# Mana

To avoid abusing of the system, storing of webpages is tracked with Mana.
For some operations you have to pay Mana, for others you receive Mana.

## Pay Mana

Before you can do one of the following operations, you have to pay Mana:

- have your webpage indexed by the trusted servers
- have another node reliably store your page for 1 minute

## Receive Mana

For the following operations, you will receive Mana:

- prove that you have another webpage stored
- serve another webpage

## Choice of nodes

The trusted servers will chose which node will either store a webpage or serve it to a request.
Serving a webpage is done in the following order:

1. the owner of the webpage, if it's still online
2. the node holding the webpage, probabilistically depending on the following parameters (this will surely be exploited somehow):
  - load: the higher the load, the smaller the probability it will be chosen
  - mana: the lower the Mana, the higher the probability it will be chosen

# Versions

Current thinking of how versions will be handled:
1. free adding of html-enabled text
  - every node downloads the last 100 texts
  - nodes need to ping regularly the trusted servers
  - the servers offer a list of most reliable nodes to download the pages from
    - the Oracle Servers put themselves at the end, so they can be used, but have lowest priority
  - the OS keep track of statistics of how many pages where downloaded by whom
  - a small editor to write the text in md-format or html
2. adding a simple javascript smart contract
  - offer callbacks to the wasm for
    - listing pages
    - requesting pages
    - storing pages
    - -> think about simple ACL
  - enable use of Mana
  - prepare for new versions of contracts and API
  - extend the editor to allow to write js-contracts
3. add a naming directory to find js-contracts
  - a js-contract can be named and retrieved by name
4. add a smart-contract callback that can be run on the OS
  - for updating the list
  - node sends request to OS, OS returns list of verifying nodes, node sends contract to nodes for verification,
    