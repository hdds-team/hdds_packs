// Take one screenshot of a viewer, once it has really received samples.
//
// Driven by tools/capture-screenshots.sh. The wait is on the data rather than
// on a timer: the page's own WebSocket is wrapped before any of its scripts
// run, and the shot is taken after `--min` samples have arrived. A page that
// never reaches the threshold is reported instead of being photographed empty.
//
// Usage: node capture-shot.mjs <url> <out.png> [--min N] [--settle S] [--max S]

const [url, out] = process.argv.slice(2);
const arg = (name, fallback) => {
    const i = process.argv.indexOf(name);
    return i > 0 ? Number(process.argv[i + 1]) : fallback;
};
const MIN = arg("--min", 40);
const SETTLE = arg("--settle", 8);
const MAX = arg("--max", 40);

const COUNTER = `
window.__samples = 0;
const Native = window.WebSocket;
function Counting(...args) {
    const socket = new Native(...args);
    socket.addEventListener('message', (event) => {
        try {
            if (JSON.parse(event.data).type === 'data') window.__samples++;
        } catch (_) { /* not a sample frame */ }
    });
    return socket;
}
Counting.prototype = Native.prototype;
Object.assign(Counting, Native);
window.WebSocket = Counting;
`;

const targets = await (await fetch("http://127.0.0.1:9222/json")).json();
const target = targets.find((t) => t.type === "page");
if (!target) {
    console.error("no page target on the DevTools endpoint");
    process.exit(2);
}

const socket = new WebSocket(target.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    const resolve = pending.get(message.id);
    if (resolve) {
        pending.delete(message.id);
        resolve(message);
    }
});
await new Promise((resolve) => socket.addEventListener("open", resolve));

const send = (method, params = {}) => new Promise((resolve) => {
    const id = nextId++;
    pending.set(id, resolve);
    socket.send(JSON.stringify({ id, method, params }));
});
const sleep = (seconds) => new Promise((r) => setTimeout(r, seconds * 1000));
const samples = async () => {
    const reply = await send("Runtime.evaluate",
        { expression: "window.__samples || 0", returnByValue: true });
    return reply?.result?.result?.value ?? 0;
};

await send("Page.enable");
await send("Emulation.setDeviceMetricsOverride",
    { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false });
await send("Page.addScriptToEvaluateOnNewDocument", { source: COUNTER });
await send("Page.navigate", { url });

let seen = 0;
const deadline = Date.now() + MAX * 1000;
while (Date.now() < deadline) {
    await sleep(1);
    seen = await samples();
    if (seen >= MIN) break;
}
const reached = seen >= MIN;
await sleep(SETTLE);
seen = await samples();

const shot = await send("Page.captureScreenshot", { format: "png" });
const encoded = shot?.result?.data;
if (!encoded) {
    console.error("no screenshot returned");
    process.exit(1);
}
const { writeFileSync } = await import("node:fs");
const bytes = Buffer.from(encoded, "base64");
writeFileSync(out, bytes);
console.log("%s  %d samples%s, %d KB",
    out.split("/").pop(), seen, reached ? "" : " (THRESHOLD NOT REACHED)",
    Math.round(bytes.length / 1024));
try { socket.close(); } catch { /* already closing */ }
process.exit(reached ? 0 : 3);
