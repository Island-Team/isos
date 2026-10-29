# Local manager page

IsOS serves a small browser interface from its own loopback bridge. It uses the same local API as Island-web. The page has no remote assets or account requirement.

[中文](../../../docs_cn/isos/usage/local-ui_cn.md)

## Open and pair

Run `isos ui` and open the `http://127.0.0.1:17600/#token=...` link it prints. The fragment is sent to the page, never to the HTTP server. The page removes it from the address bar and keeps the token only in tab memory. Closing the tab or stopping `isos ui` ends that pairing. You can also open the base URL and paste the token printed in the terminal. For a different port, run `isos ui --port 17601` and use its printed link.

Only a local browser connecting to the selected `127.0.0.1` port can load the page. API calls require the bearer token. The bridge continues to accept the hosted Island-web origin; its pairing flow is unchanged.

## Manage the deployment

Overview shows EOS, EGIA, and active nodes. The Start and Stop buttons call the scoped IsOS lifecycle API; component buttons address EOS or EGIA and use the dependency rules of the deployment graph. Choose a deployment name at the top and Refresh to load its state.

Packages lists installed receipts and searches the approved catalog. Click a row for its details. The page shows an `isos packages install` command for catalog results; installing and removing packages remain CLI operations in this compact interface.

Config graph displays nodes and edges. Create an empty graph or load the selected deployment, add a disabled node, edit a node's JSON, or edit the full graph JSON including edges. Apply validates and saves it through IsOS. An invalid graph produces the server's validation message. Apply does not restart running nodes; use the CLI `isos restart` when ready to activate the new snapshot.

Related: [local API](../interfaces/local-api.md), [deployment graph](../interfaces/deployment-graph.md).
