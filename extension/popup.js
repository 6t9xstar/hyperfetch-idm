const api = globalThis.browser ?? globalThis.chrome;

const elements = {
  status: document.getElementById("status"),
  statusText: document.getElementById("status-text"),
  pairing: document.getElementById("pairing"),
  content: document.getElementById("content"),
  bridgeUrl: document.getElementById("bridge-url"),
  bridgeToken: document.getElementById("bridge-token"),
  savePairing: document.getElementById("save-pairing"),
  pairingError: document.getElementById("pairing-error"),
  pageTitle: document.getElementById("page-title"),
  pageUrl: document.getElementById("page-url"),
  manualUrl: document.getElementById("manual-url"),
  sendManual: document.getElementById("send-manual"),
  rescan: document.getElementById("rescan"),
  sendAll: document.getElementById("send-all"),
  links: document.getElementById("links"),
  empty: document.getElementById("empty"),
  editPairing: document.getElementById("edit-pairing"),
  toast: document.getElementById("toast"),
};

const state = {
  config: null,
  page: null,
  links: [],
  sent: new Set(),
};

let toastTimer = null;

function sendMessage(message) {
  return new Promise((resolve) => {
    try {
      const maybe = api.runtime.sendMessage(message);
      if (maybe && typeof maybe.then === "function") {
        maybe.then(resolve).catch((error) => resolve({ ok: false, error: String(error) }));
        return;
      }
    } catch {
      /* fall through */
    }
    try {
      api.runtime.sendMessage(message, (response) => {
        const lastError = globalThis.chrome?.runtime?.lastError;
        if (lastError) resolve({ ok: false, error: lastError.message });
        else resolve(response ?? { ok: false, error: "No response from background worker." });
      });
    } catch (error) {
      resolve({ ok: false, error: String(error) });
    }
  });
}

function tabsQuery(query) {
  return new Promise((resolve) => {
    try {
      const maybe = api.tabs.query(query);
      if (maybe && typeof maybe.then === "function") {
        maybe.then(resolve).catch(() => resolve([]));
        return;
      }
    } catch {
      /* fall through */
    }
    try {
      api.tabs.query(query, (tabs) => resolve(tabs ?? []));
    } catch {
      resolve([]);
    }
  });
}

function setStatus(mode, text) {
  elements.status.classList.remove("status-online", "status-offline");
  elements.status.classList.add(mode === "online" ? "status-online" : "status-offline");
  elements.statusText.textContent = text;
}

function showToast(message, isError = false) {
  elements.toast.textContent = message;
  elements.toast.classList.toggle("toast-error", isError);
  elements.toast.classList.remove("hidden");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => elements.toast.classList.add("hidden"), 4500);
}

function showPairing(message) {
  elements.pairing.classList.remove("hidden");
  elements.content.classList.add("hidden");
  if (state.config) {
    elements.bridgeUrl.value = state.config.bridgeUrl ?? "";
    elements.bridgeToken.value = state.config.token ?? "";
  }
  elements.pairingError.textContent = message ?? "";
  elements.pairingError.classList.toggle("hidden", !message);
}

function showContent() {
  elements.pairing.classList.add("hidden");
  elements.content.classList.remove("hidden");
}

function kindFor(url) {
  const lower = String(url).toLowerCase();
  if (lower.includes(".m3u8")) return "hls";
  if (lower.includes(".mpd")) return "dash";
  return "download";
}

function filenameFor(url) {
  try {
    const parsed = new URL(url);
    const raw = parsed.pathname.split("/").filter(Boolean).pop() || "";
    return raw ? decodeURIComponent(raw) : null;
  } catch {
    return null;
  }
}

function validUrl(value) {
  try {
    const parsed = new URL(value);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

function renderLinks() {
  elements.links.innerHTML = "";
  elements.empty.classList.toggle("hidden", state.links.length > 0);

  for (const link of state.links) {
    const item = document.createElement("li");

    const kind = document.createElement("span");
    kind.className = `kind kind-${link.kind}`;
    kind.textContent = link.kind === "hls" ? "HLS" : link.kind === "dash" ? "DASH" : "FILE";
    item.appendChild(kind);

    const main = document.createElement("div");
    main.className = "link-main";
    const name = document.createElement("p");
    name.className = "link-name";
    name.textContent = link.filename || filenameFor(link.url) || link.url;
    name.title = link.url;
    const url = document.createElement("p");
    url.className = "link-url";
    url.textContent = link.url;
    url.title = link.url;
    main.appendChild(name);
    main.appendChild(url);
    item.appendChild(main);

    const button = document.createElement("button");
    button.className = "send-one";
    button.textContent = state.sent.has(link.url) ? "Sent" : "Send";
    button.classList.toggle("sent", state.sent.has(link.url));
    button.addEventListener("click", () => void sendLink(link, button));
    item.appendChild(button);

    elements.links.appendChild(item);
  }
}

async function sendLink(link, button) {
  button.disabled = true;
  const response = await sendMessage({
    type: "hf-capture",
    payload: {
      url: link.url,
      filename: link.filename || filenameFor(link.url),
      kind: link.kind || kindFor(link.url),
      pageUrl: state.page?.pageUrl ?? null,
      referer: state.page?.pageUrl ?? null,
    },
  });
  button.disabled = false;

  if (!response.ok) {
    showToast(response.error || "Capture failed.", true);
    return;
  }

  state.sent.add(link.url);
  button.textContent = "Sent";
  button.classList.add("sent");
  const pending = response.result?.status === "pending";
  showToast(
    pending
      ? "Sent to HyperFetch — confirm it in the desktop app."
      : "Download queued in HyperFetch.",
  );
}

async function scanPage() {
  setStatus("online", "Online");
  const response = await sendMessage({ type: "hf-scan" });
  if (!response.ok) {
    state.links = [];
    renderLinks();
    const tabs = await tabsQuery({ active: true, currentWindow: true });
    const tab = tabs[0];
    state.page = {
      pageUrl: tab?.url ?? "",
      title: tab?.title ?? "Current page",
    };
    elements.pageTitle.textContent = state.page.title;
    elements.pageUrl.textContent = state.page.pageUrl;
    showToast(response.error || "Could not scan this page.", true);
    return;
  }
  state.page = response.data;
  state.links = response.data.links ?? [];
  state.sent = new Set();
  elements.pageTitle.textContent = response.data.title || "Current page";
  elements.pageUrl.textContent = response.data.pageUrl || "";
  renderLinks();
}

async function refresh() {
  const configResponse = await sendMessage({ type: "hf-config-get" });
  state.config = configResponse.config ?? null;

  if (!state.config || !state.config.token) {
    setStatus("offline", "Unpaired");
    showPairing();
    return;
  }

  const test = await sendMessage({ type: "hf-test", config: state.config });
  if (!test.ok) {
    setStatus("offline", "Offline");
    showPairing(test.error ?? "HyperFetch is not reachable.");
    return;
  }

  setStatus("online", "Online");
  showContent();
  await scanPage();
}

elements.savePairing.addEventListener("click", async () => {
  elements.pairingError.classList.add("hidden");
  const bridgeUrl = elements.bridgeUrl.value.trim() || "http://127.0.0.1:47821";
  const token = elements.bridgeToken.value.trim();
  if (!token) {
    elements.pairingError.textContent = "Paste the bridge token from HyperFetch settings.";
    elements.pairingError.classList.remove("hidden");
    return;
  }
  if (!validUrl(bridgeUrl)) {
    elements.pairingError.textContent = "Bridge URL must start with http:// or https://.";
    elements.pairingError.classList.remove("hidden");
    return;
  }

  elements.savePairing.disabled = true;
  await sendMessage({ type: "hf-config-set", config: { bridgeUrl, token } });
  const test = await sendMessage({ type: "hf-test", config: { bridgeUrl, token } });
  elements.savePairing.disabled = false;

  if (!test.ok) {
    elements.pairingError.textContent = test.error ?? "Connection failed.";
    elements.pairingError.classList.remove("hidden");
    setStatus("offline", "Offline");
    return;
  }

  state.config = { bridgeUrl, token };
  setStatus("online", "Online");
  showContent();
  await scanPage();
});

elements.editPairing.addEventListener("click", () => {
  showPairing();
});

elements.rescan.addEventListener("click", () => void scanPage());

elements.sendAll.addEventListener("click", async () => {
  const pending = state.links.filter((link) => !state.sent.has(link.url));
  if (pending.length === 0) {
    showToast("Nothing new to send.");
    return;
  }
  elements.sendAll.disabled = true;
  let count = 0;
  for (const link of pending) {
    const response = await sendMessage({
      type: "hf-capture",
      payload: {
        url: link.url,
        filename: link.filename || filenameFor(link.url),
        kind: link.kind || kindFor(link.url),
        pageUrl: state.page?.pageUrl ?? null,
        referer: state.page?.pageUrl ?? null,
      },
    });
    if (response.ok) {
      state.sent.add(link.url);
      count += 1;
    }
  }
  elements.sendAll.disabled = false;
  renderLinks();
  showToast(
    count > 0 ? `Sent ${count} link${count === 1 ? "" : "s"} to HyperFetch.` : "No links were sent.",
    count === 0,
  );
});

elements.sendManual.addEventListener("click", async () => {
  const url = elements.manualUrl.value.trim();
  if (!validUrl(url)) {
    showToast("Enter a valid http(s) URL.", true);
    return;
  }
  elements.sendManual.disabled = true;
  const response = await sendMessage({
    type: "hf-capture",
    payload: {
      url,
      filename: filenameFor(url),
      kind: kindFor(url),
      pageUrl: state.page?.pageUrl ?? null,
      referer: state.page?.pageUrl ?? null,
    },
  });
  elements.sendManual.disabled = false;
  if (!response.ok) {
    showToast(response.error || "Capture failed.", true);
    return;
  }
  elements.manualUrl.value = "";
  showToast(
    response.result?.status === "pending"
      ? "Sent to HyperFetch — confirm it in the desktop app."
      : "Download queued in HyperFetch.",
  );
});

elements.manualUrl.addEventListener("keydown", (event) => {
  if (event.key === "Enter") elements.sendManual.click();
});

void refresh();
