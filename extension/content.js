(() => {
  const MEDIA_EXTENSIONS = new Set([
    "m3u8",
    "mpd",
    "mp4",
    "mkv",
    "webm",
    "mov",
    "avi",
    "flv",
    "m4v",
    "wmv",
    "mpg",
    "mpeg",
    "ts",
    "mp3",
    "m4a",
    "aac",
    "flac",
    "wav",
    "ogg",
    "opus",
    "wma",
    "zip",
    "rar",
    "7z",
    "tar",
    "gz",
    "bz2",
    "xz",
    "zst",
    "iso",
    "pdf",
    "epub",
    "mobi",
    "doc",
    "docx",
    "xls",
    "xlsx",
    "ppt",
    "pptx",
    "exe",
    "msi",
    "msix",
    "appx",
    "apk",
    "dmg",
    "pkg",
    "deb",
    "rpm",
    "jpg",
    "jpeg",
    "png",
    "gif",
    "webp",
    "svg",
    "heic",
  ]);

  const STREAM_EXTENSIONS = new Set(["m3u8", "mpd"]);

  function extensionOf(url) {
    try {
      const parsed = new URL(url, location.href);
      const match = parsed.pathname.match(/\.([a-z0-9]{1,8})$/i);
      return match ? match[1].toLowerCase() : "";
    } catch {
      return "";
    }
  }

  function kindFor(url) {
    const extension = extensionOf(url);
    if (extension === "m3u8") return "hls";
    if (extension === "mpd") return "dash";
    if (STREAM_EXTENSIONS.has(extension)) return "download";
    return extension ? "download" : "download";
  }

  function filenameFor(url) {
    try {
      const parsed = new URL(url, location.href);
      const name = decodeURIComponent(parsed.pathname.split("/").filter(Boolean).pop() || "");
      return name || null;
    } catch {
      return null;
    }
  }

  function isCandidate(url) {
    const extension = extensionOf(url);
    return MEDIA_EXTENSIONS.has(extension) || extension === "m3u8" || extension === "mpd";
  }

  function scan() {
    const urls = new Set();

    const add = (raw) => {
      if (!raw) return;
      const value = String(raw).trim();
      if (!value || value.startsWith("blob:") || value.startsWith("data:")) return;
      let absolute;
      try {
        absolute = new URL(value, location.href).href;
      } catch {
        return;
      }
      if (!/^https?:/i.test(absolute)) return;
      if (!isCandidate(absolute)) return;
      urls.add(absolute);
    };

    document.querySelectorAll("video, audio").forEach((element) => {
      add(element.currentSrc);
      add(element.src);
      const poster = element.getAttribute("poster");
      if (poster) add(poster);
    });

    document.querySelectorAll("source").forEach((element) => add(element.src));
    document.querySelectorAll("track").forEach((element) => add(element.src));
    document.querySelectorAll("a[href]").forEach((element) => add(element.href));
    document.querySelectorAll("link[href]").forEach((element) => add(element.href));

    document.querySelectorAll("[data-src], [data-url], [data-video]").forEach((element) => {
      add(element.getAttribute("data-src"));
      add(element.getAttribute("data-url"));
      add(element.getAttribute("data-video"));
    });

    try {
      performance.getEntriesByType("resource").forEach((entry) => {
        const name = entry && entry.name;
        if (name && /\.(m3u8|mpd)(\?|#|$)/i.test(name)) add(name);
      });
    } catch {
      /* performance API may be unavailable */
    }

    for (const video of document.querySelectorAll("video")) {
      if (video.src && video.src.startsWith("blob:")) {
        const source = video.querySelector("source[src]");
        if (source) add(source.src);
      }
    }

    const links = Array.from(urls).slice(0, 200).map((url) => ({
      url,
      filename: filenameFor(url),
      kind: kindFor(url),
    }));

    return { pageUrl: location.href, title: document.title || location.hostname, links };
  }

  if (globalThis.__hyperfetchInjected) return;
  globalThis.__hyperfetchInjected = true;

  const runtime = globalThis.browser?.runtime ?? globalThis.chrome?.runtime;
  if (!runtime) return;

  runtime.onMessage.addListener((message, _sender, sendResponse) => {
    if (!message || message.type !== "hf-scan") return undefined;
    try {
      sendResponse({ ok: true, data: scan() });
    } catch (error) {
      sendResponse({ ok: false, error: String(error) });
    }
    return undefined;
  });
})();
