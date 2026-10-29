"use strict";

const $ = (id) => document.getElementById(id);
const text = (id, value) => { $(id).textContent = value; };
const state = { token: "", language: "en", graph: null, selectedNode: null, noticeTimer: null };
const labels = {
  en: { subtitle:"Local manager",unpaired:"Not paired",paired:"Paired",pairing:"PAIRING",pairTitle:"Connect to this IsOS session",pairHelp:"Open the local link printed by isos ui, or paste its one-time token here. The token stays in this tab only.",token:"Pairing token",connect:"Connect",workspace:"WORKSPACE",heading:"Your local deployment",deployment:"Deployment",refresh:"Refresh",overview:"Overview",packages:"Packages",graph:"Config graph",runtime:"Runtime",runtimeHelp:"Start or stop the deployment and its components.",startAll:"Start stack",stopAll:"Stop stack",eosHelp:"Core execution backend",egiaHelp:"Agent and gateway backend",start:"Start",stop:"Stop",runningNodes:"Deployment nodes",packageLibrary:"Package library",packageHelp:"Installed releases are managed by IsOS. Search the approved catalog, then install with the CLI.",installed:"Installed",catalog:"Approved catalog",search:"Search",details:"DETAILS",choosePackage:"Choose a package",graphTitle:"Config graph",graphHelp:"Edit nodes and edges, then apply the complete graph. Running nodes keep their current snapshot until restart.",newGraph:"New graph",apply:"Apply graph",nodes:"Nodes",addNode:"Add node",edges:"Edges",jsonEditor:"Graph JSON",format:"Format",jsonHelp:"Full schema; edit edges here. Click a node to edit its JSON.",delete:"Delete",saveNode:"Save node",footer:"Local session · no cloud account required",offline:"Stopped",online:"Running",none:"None yet",invalidToken:"Enter the 64-character token printed by isos ui.",badGraph:"Graph JSON is invalid",noGraph:"No graph applied. Choose New graph to begin.",saved:"Graph applied. Running nodes still use the previous snapshot until restarted.",missingDeployment:"Use a lowercase deployment name, up to 48 characters.",missingRole:"EOS and EGIA component nodes already exist.",selectNode:"Select a node first.",searchPrompt:"Search the approved catalog above.",searchEmpty:"No matching packages.",installHint:"Install from a terminal:",unavailable:"Unavailable" },
  cn: { subtitle:"本地管理器",unpaired:"未配对",paired:"已配对",pairing:"配对",pairTitle:"连接本次 IsOS 会话",pairHelp:"打开 isos ui 输出的本地链接，或在此粘贴一次性令牌。令牌仅保存在当前标签页内。",token:"配对令牌",connect:"连接",workspace:"工作区",heading:"本地部署",deployment:"部署",refresh:"刷新",overview:"概览",packages:"软件包",graph:"配置图",runtime:"运行状态",runtimeHelp:"启动或停止部署及其组件。",startAll:"启动整套",stopAll:"停止整套",eosHelp:"核心执行后端",egiaHelp:"代理与网关后端",start:"启动",stop:"停止",runningNodes:"部署节点",packageLibrary:"软件包库",packageHelp:"IsOS 管理已安装版本。搜索审核目录后通过命令行安装。",installed:"已安装",catalog:"审核目录",search:"搜索",details:"详情",choosePackage:"选择软件包",graphTitle:"配置图",graphHelp:"编辑节点与边，再应用完整配置图。运行中的节点会保留当前快照，直到重新启动。",newGraph:"新建配置图",apply:"应用配置图",nodes:"节点",addNode:"添加节点",edges:"边",jsonEditor:"配置图 JSON",format:"格式化",jsonHelp:"完整格式；在此编辑边。点击节点可编辑其 JSON。",delete:"删除",saveNode:"保存节点",footer:"本地会话 · 无需云端账号",offline:"已停止",online:"运行中",none:"暂无",invalidToken:"请输入 isos ui 输出的 64 位令牌。",badGraph:"配置图 JSON 无效",noGraph:"尚未应用配置图。选择新建配置图开始。",saved:"配置图已应用。运行中的节点仍使用原快照，直到重新启动。",missingDeployment:"部署名须为小写字母开头，最长 48 字符。",missingRole:"EOS 和 EGIA 组件节点已存在。",selectNode:"请先选择节点。",searchPrompt:"在上方搜索审核目录。",searchEmpty:"没有匹配的软件包。",installHint:"在终端安装：",unavailable:"不可用" }
};
function t(key) { return labels[state.language][key] || key; }
function notice(message, error=false) {
  const box = $("notice"); box.textContent = message; box.classList.toggle("error", error); box.classList.add("show");
  clearTimeout(state.noticeTimer); state.noticeTimer = setTimeout(() => box.classList.remove("show"), 7000);
}
function setLanguage(language) {
  state.language = language; document.documentElement.lang = language === "cn" ? "zh-CN" : "en";
  document.querySelectorAll("[data-i18n]").forEach((node) => { node.textContent = t(node.dataset.i18n); });
  $("language").textContent = language === "en" ? "中文" : "English";
  if (state.token) refreshStatus();
}
function deployment() {
  const value = $("deployment").value.trim();
  if (!/^[a-z][a-z0-9-]{0,47}$/.test(value)) throw new Error(t("missingDeployment"));
  return value;
}
async function api(path, options={}) {
  const response = await fetch("/api/isos/v1/" + path, {
    method: options.body ? "POST" : "GET", cache:"no-store", credentials:"omit", referrerPolicy:"no-referrer",
    headers: { Authorization:"Bearer " + state.token, ...(options.body ? {"Content-Type":"application/json"} : {}) },
    body: options.body ? JSON.stringify(options.body) : undefined
  });
  let envelope;
  try { envelope = await response.json(); } catch { throw new Error("IsOS returned an invalid response"); }
  if (!response.ok || !envelope.ok) throw new Error(envelope.error || `HTTP ${response.status}`);
  return envelope.data;
}
function row(label, meta, right) {
  const element = document.createElement("div"); element.className = "row";
  const left = document.createElement("div"); const title = document.createElement("span"); title.className="row-title"; title.textContent=label;
  const extra = document.createElement("small"); extra.textContent=meta || ""; left.append(title, document.createElement("br"), extra);
  const badge = document.createElement("span"); badge.className="tag"; badge.textContent=right || ""; element.append(left,badge); return element;
}
function empty(target, message) { target.replaceChildren(row(message, "", "")); }
async function connect(token) {
  if (!/^[0-9a-f]{64}$/i.test(token)) { notice(t("invalidToken"),true); return; }
  state.token = token.toLowerCase();
  try { await api("packages"); }
  catch(error) { state.token=""; notice(error.message,true); return; }
  $("tokenInput").value=""; $("pairing").hidden=true; $("workspace").hidden=false;
  $("connection").classList.add("online"); text("connection",t("paired"));
  await Promise.allSettled([refreshStatus(),refreshPackages(),loadGraph()]);
}
async function refreshStatus() {
  try {
    const name = deployment();
    const statuses = await Promise.all([api("status?deployment="+encodeURIComponent(name)),api("status?deployment="+encodeURIComponent(name)+"&role=eos"),api("status?deployment="+encodeURIComponent(name)+"&role=egia")]);
    const [all,eos,egia]=statuses;
    for (const [role,data] of [["eos",eos],["egia",egia]]) { const label=$(role+"State"); label.textContent=data.running?t("online"):t("offline"); label.classList.toggle("running",!!data.running); }
    text("activeName",all.active || name);
    const target=$("runtimeNodes"); target.replaceChildren();
    if (!all.nodes?.length) empty(target,t("none"));
    else all.nodes.forEach((node) => target.append(row(node.id,"",node.running?t("online"):t("offline"))));
  } catch(error) { notice(error.message,true); }
}
async function runAction(action,role) {
  try { const name=deployment(); const data=await api(action,{body:{schema_version:1,deployment:name,...(role?{role}:{})}}); notice(JSON.stringify(data)); await refreshStatus(); }
  catch(error) { notice(error.message,true); }
}
function packageList(data) {
  if (Array.isArray(data)) return data;
  if (data && typeof data === "object") return data.items || data.packages || data.results || data.releases || [];
  return [];
}
function packageButton(item, target, catalog=false) {
  const button=row(item.id || item.name || item.slug || "package",item.description || item.version || "",catalog?(item.version || item.latest_version || ""):item.version);
  button.classList.add("row-button"); button.type="button";
  const wrapper=document.createElement("button"); wrapper.type="button"; wrapper.className="row row-button";
  while(button.firstChild) wrapper.append(button.firstChild);
  wrapper.addEventListener("click", () => {
    const id=item.id || item.name || item.slug || "";
    text("detailTitle",id || t("details"));
    const command=catalog && id ? `${t("installHint")}\nisos packages install ${id} --version ${item.version || item.latest_version || "latest"}\n\n` : "";
    text("packageDetail",command+JSON.stringify(item,null,2));
  }); target.append(wrapper);
}
async function refreshPackages() {
  try { const receipts=await api("packages"); const target=$("installedList"); target.replaceChildren(); text("packageCount",String(receipts.length)); if (!receipts.length) empty(target,t("none")); else receipts.forEach((receipt)=>packageButton(receipt,target)); }
  catch(error) { notice(error.message,true); }
}
async function searchCatalog() {
  try { const query=$("searchQuery").value.trim(); const data=await api("catalog?q="+encodeURIComponent(query)); const target=$("catalogList"); target.replaceChildren(); const items=packageList(data); if (!items.length) empty(target,t("searchEmpty")); else items.forEach((item)=>packageButton(item,target,true)); }
  catch(error) { notice(error.message,true); }
}
function currentGraph() {
  let graph; try { graph=JSON.parse($("graphJson").value); } catch { throw new Error(t("badGraph")); }
  if (!graph || typeof graph!=="object" || !Array.isArray(graph.nodes) || !Array.isArray(graph.edges)) throw new Error(t("badGraph"));
  return graph;
}
function showGraph() {
  let graph; try { graph=currentGraph(); } catch { return; }
  const nodes=$("graphNodes"), edges=$("graphEdges"); nodes.replaceChildren(); edges.replaceChildren();
  if (!graph.nodes.length) empty(nodes,t("none"));
  graph.nodes.forEach((node,index)=>{
    const button=document.createElement("button"); button.type="button"; button.className="node-card";
    const name=document.createElement("strong"); name.textContent=node.id || `node ${index+1}`;
    const info=document.createElement("small"); info.textContent=[node.kind,node.role,node.enabled===false?t("offline"):""].filter(Boolean).join(" · ");
    button.append(name,info); button.addEventListener("click",()=>{
      state.selectedNode=index; $("nodeInspector").hidden=false; text("nodeTitle",node.id || "Node"); $("nodeJson").value=JSON.stringify(node,null,2);
    }); nodes.append(button);
  });
  if (!graph.edges.length) empty(edges,t("none"));
  graph.edges.forEach((edge)=>edges.append(row(`${edge.from} → ${edge.to}`,edge.kind,"")));
}
function writeGraph(graph) { state.graph=graph; $("graphJson").value=JSON.stringify(graph,null,2); $("nodeInspector").hidden=true; state.selectedNode=null; showGraph(); }
async function loadGraph() {
  try { const graph=await api("graph?deployment="+encodeURIComponent(deployment())); writeGraph(graph); }
  catch(error) { $("graphJson").value=""; $("graphNodes").replaceChildren(); $("graphEdges").replaceChildren(); notice(error.message || t("noGraph"),true); }
}
function newGraph() { try { writeGraph({schema_version:1,name:deployment(),nodes:[],edges:[]}); } catch(error) { notice(error.message,true); } }
function addNode() {
  try {
    const graph=currentGraph(),kind=$("nodeKind").value;
    const used=new Set(graph.nodes.map((node)=>node.id)); let id=kind;
    if (kind==="component") { id=["eos","egia"].find((name)=>!graph.nodes.some((node)=>node.role===name)); if (!id) throw new Error(t("missingRole")); }
    else { let n=2; while(used.has(id)) id=`${kind}-${n++}`; }
    const node={id,kind,enabled:false,owned:true,settings:{}};
    if(kind==="component") node.role=id;
    graph.nodes.push(node); writeGraph(graph); state.selectedNode=graph.nodes.length-1; $("nodeInspector").hidden=false; text("nodeTitle",id); $("nodeJson").value=JSON.stringify(node,null,2);
  } catch(error) { notice(error.message,true); }
}
function saveNode() {
  try { const graph=currentGraph(); if(state.selectedNode===null || !graph.nodes[state.selectedNode]) throw new Error(t("selectNode")); const node=JSON.parse($("nodeJson").value); graph.nodes[state.selectedNode]=node; writeGraph(graph); }
  catch(error) { notice(error.message,true); }
}
function deleteNode() {
  try { const graph=currentGraph(); if(state.selectedNode===null) throw new Error(t("selectNode")); const [removed]=graph.nodes.splice(state.selectedNode,1); graph.edges=graph.edges.filter((edge)=>edge.from!==removed.id && edge.to!==removed.id); writeGraph(graph); }
  catch(error) { notice(error.message,true); }
}
async function applyGraph() {
  try { const graph=currentGraph(),name=deployment(); if(graph.name!==name) throw new Error(`Graph name ${graph.name} differs from deployment ${name}`); await api("apply",{body:{schema_version:1,deployment:name,graph}}); writeGraph(graph); notice(t("saved")); await refreshStatus(); }
  catch(error) { notice(error.message,true); }
}
function init() {
  const fragment=location.hash.startsWith("#")?new URLSearchParams(location.hash.slice(1)):null;
  const token=fragment?.get("token");
  if(location.hash) history.replaceState(null,"",location.pathname+location.search);
  $("pairForm").addEventListener("submit",(event)=>{event.preventDefault(); connect($("tokenInput").value.trim());});
  $("language").addEventListener("click",()=>setLanguage(state.language==="en"?"cn":"en"));
  document.querySelectorAll(".tab").forEach((button)=>button.addEventListener("click",()=>{document.querySelectorAll(".tab").forEach((tab)=>tab.classList.toggle("active",tab===button));document.querySelectorAll(".page").forEach((page)=>page.hidden=page.id!==button.dataset.tab);}));
  $("reload").addEventListener("click",()=>Promise.allSettled([refreshStatus(),refreshPackages(),loadGraph()]));
  $("deployment").addEventListener("change",()=>Promise.allSettled([refreshStatus(),loadGraph()]));
  $("reloadPackages").addEventListener("click",refreshPackages);
  $("searchForm").addEventListener("submit",(event)=>{event.preventDefault();searchCatalog();});
  $("startAll").addEventListener("click",()=>runAction("start")); $("stopAll").addEventListener("click",()=>runAction("stop"));
  document.querySelectorAll("[data-action]").forEach((button)=>button.addEventListener("click",()=>runAction(button.dataset.action,button.dataset.role)));
  $("newGraph").addEventListener("click",newGraph); $("applyGraph").addEventListener("click",applyGraph);
  $("formatGraph").addEventListener("click",()=>{try{writeGraph(currentGraph());}catch(error){notice(error.message,true);}});
  $("graphJson").addEventListener("input",showGraph); $("addNode").addEventListener("click",addNode);
  $("saveNode").addEventListener("click",saveNode); $("deleteNode").addEventListener("click",deleteNode);
  empty($("catalogList"),t("searchPrompt"));
  if(token) connect(token);
}
init();
