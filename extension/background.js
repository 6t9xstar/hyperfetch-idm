const api = globalThis.browser ?? globalThis.chrome;
const isFirefox = Boolean(globalThis.browser);

const DEFAULT_CONFIG = {
  bridgeUrl: "http://127.0.0.1:47821",
  token: "",
};

function storageGet(key) {
  return new Promise((resolve) => {
    try {
      const maybe = api.storage.local.get(key);
      if (maybe && typeof maybe.then === "function") {
        maybe.then(resolve).catch(() => resolve({}));
        return;
      }
    } catch {
      /* fall through to callback form */
    }
    try {
      api.storage.local.get(key, (value) => resolve(value ?? {}));
    } catch {
      resolve({});
    }
  });
}

function storageSet(value) {
  return new Promise((resolve) => {
    try {
      const maybe = api.storage.local.set(value);
      if (maybe && typeof maybe.then === "function") {
        maybe.then(resolve).catch(resolve);
        return;
      }
    } catch {
      /* fall through to callback form */
    }
    try {
      api.storage.local.set(value, resolve);
    } catch {
      resolve();
    }
  });
}

async function getConfig() {
  const stored = await storageGet("hyperfetch");
  const config = stored?.hyperfetch ?? {};
  return {
    bridgeUrl: (config.bridgeUrl || DEFAULT_CONFIG.bridgeUrl).replace(/\/+$/, ""),
    token: config.token || "",
  };
}

async function setConfig(config) {
  await storageSet({
    hyperfetch: {
      bridgeUrl: (config.bridgeUrl || DEFAULT_CONFIG.bridgeUrl).replace(/\/+$/, ""),
      token: config.token || "",
    },
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

function sendTabMessage(tabId, message) {
  return new Promise((resolve, reject) => {
    try {
      const maybe = api.tabs.sendMessage(tabId, message);
      if (maybe && typeof maybe.then === "function") {
        maybe.then(resolve).catch(reject);
        return;
      }
    } catch (error) {
      reject(error);
      return;
    }
    try {
      api.tabs.sendMessage(tabId, message, (response) => {
        const lastError = globalThis.chrome?.runtime?.lastError;
        if (lastError) reject(new Error(lastError.message));
        else resolve(response);
      });
    } catch (error) {
      reject(error);
    }
  });
}

async function cookiesFor(url) {
  try {
    if (isFirefox) {
      const list = await api.cookies.getAll({ url });
      return (list ?? []).map((cookie) => `${cookie.name}=${cookie.value}`).join("; ");
    }
    return await new Promise((resolve) => {
      try {
        api.cookies.getAll({ url }, (list) => {
          resolve((list ?? []).map((cookie) => `${cookie.name}=${cookie.value}`).join("; "));
        });
      } catch {
        resolve("");
      }
    });
  } catch {
    return "";
  }
}

async function bridgeRequest(path, options = {}) {
  const config = await getConfig();
  if (!config.token) {
    throw new Error("Extension is not paired yet. Open HyperFetch settings and copy the bridge token.");
  }
  const response = await fetch(`${config.bridgeUrl}${path}`, {
    ...options,
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${config.token}`,
      ...(options.headers ?? {}),
    },
  });
  const data = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(data.error || `Bridge returned HTTP ${response.status}`);
  }
  return data;
}

async function sendCapture(payload) {
  const cookie = payload.cookie ?? (await cookiesFor(payload.url));
  return bridgeRequest("/v1/capture", {
    method: "POST",
    body: JSON.stringify({
      url: payload.url,
      filename: payload.filename || null,
      referer: payload.referer || null,
      page_url: payload.pageUrl || null,
      kind: payload.kind || null,
      cookie: cookie || null,
      user_agent: navigator.userAgent,
    }),
  });
}

function flash(text) {
  try {
    const maybe = api.action?.setBadgeText?.({ text });
    if (maybe && typeof maybe.catch === "function") maybe.catch(() => {});
    if (text) {
      setTimeout(() => {
        try {
          const cleared = api.action?.setBadgeText?.({ text: "" });
          if (cleared && typeof cleared.catch === "function") cleared.catch(() => {});
        } catch {
          /* ignore */
        }
      }, 4000);
    }
  } catch {
    /* badge is cosmetic */
  }
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

function createMenus() {
  try {
    api.contextMenus.removeAll(() => {
      api.contextMenus.create({
        id: "hyperfetch-download",
        title: "Download with HyperFetch",
        contexts: ["link", "video", "audio", "image"],
      });
    });
  } catch {
    /* context menus may be unavailable */
  }
}

api.runtime.onInstalled.addListener(() => {
  createMenus();
});

api.runtime.onStartup?.addListener(() => {
  createMenus();
});

api.contextMenus.onClicked.addListener(async (info) => {
  if (info.menuItemId !== "hyperfetch-download") return;
  const url = info.linkUrl || info.srcUrl;
  if (!url) return;
  try {
    await sendCapture({
      url,
      filename: filenameFor(url),
      referer: info.pageUrl,
      pageUrl: info.pageUrl,
      kind: kindFor(url),
    });
    flash("✓");
  } catch (error) {
    flash("!");
    console.warn("HyperFetch capture failed:", error);
  }
});

async function testBridge(config) {
  const bridgeUrl = (config?.bridgeUrl || (await getConfig()).bridgeUrl).replace(/\/+$/, "");
  let health;
  try {
    const response = await fetch(`${bridgeUrl}/v1/health`);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    health = await response.json();
  } catch (error) {
    throw new Error(`Cannot reach HyperFetch at ${bridgeUrl}. Is the desktop app running?`);
  }
  const effective = {
    bridgeUrl,
    token: config?.token || (await getConfig()).token,
  };
  if (!effective.token) throw new Error("A bridge token is required.");
  const response = await fetch(`${bridgeUrl}/v1/captures`, {
    headers: { Authorization: `Bearer ${effective.token}` },
  });
  if (response.status === 401) {
    throw new Error("Token rejected. Copy the current token from HyperFetch settings.");
  }
  if (!response.ok) throw new Error(`Bridge returned HTTP ${response.status}`);
  return { app: health.app, version: health.version, bridgeUrl };
}

api.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (!message || typeof message.type !== "string") return undefined;

  (async () => {
    switch (message.type) {
      case "hf-config-get": {
        return { ok: true, config: await getConfig() };
      }
      case "hf-config-set": {
        await setConfig(message.config ?? {});
        return { ok: true, config: await getConfig() };
      }
      case "hf-test": {
        try {
          return { ok: true, info: await testBridge(message.config) };
        } catch (error) {
          return { ok: false, error: error.message };
        }
      }
      case "hf-capture": {
        try {
          const result = await sendCapture(message.payload ?? {});
          flash("✓");
          return { ok: true, result };
        } catch (error) {
          flash("!");
          return { ok: false, error: error.message };
        }
      }
      case "hf-scan": {
        const tabs = await tabsQuery({ active: true, currentWindow: true });
        const tab = tabs[0];
        if (!tab || !tab.id) return { ok: false, error: "No active tab." };
        if (!/^https?:/i.test(tab.url ?? "")) {
          return { ok: false, error: "Open a regular web page to scan for links." };
        }
        try {
          const response = await sendTabMessage(tab.id, { type: "hf-scan" });
          if (!response || response.ok === false) {
            return { ok: false, error: response?.error ?? "Could not scan this page." };
          }
          return { ok: true, data: response.data };
        } catch {
          return {
            ok: false,
            error: "Could not reach the page. Reload it and try again.",
          };
        }
      }
      default:
        return { ok: false, error: "Unknown request." };
    }
  })()
    .then(sendResponse)
    .catch((error) => sendResponse({ ok: false, error: String(error) }));

  return true;
});
