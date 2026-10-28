import { useStore } from "../store";
import { Modal } from "./Modal";
import { DownloadIcon, GlobeIcon, TrashIcon, XIcon } from "./Icons";
import { domainOf } from "../lib/format";

const KIND_STYLE: Record<string, string> = {
  hls: "border-accent-500/30 bg-accent-500/10 text-accent-300",
  dash: "border-amber-500/30 bg-amber-500/10 text-amber-300",
  download: "border-sky-500/30 bg-sky-500/10 text-sky-300",
};

export function CapturesDialog() {
  const open = useStore((state) => state.capturesOpen);
  const setOpen = useStore((state) => state.setCapturesOpen);
  const captures = useStore((state) => state.captures);
  const accept = useStore((state) => state.acceptCapture);
  const dismiss = useStore((state) => state.dismissCapture);

  return (
    <Modal
      open={open}
      onClose={() => setOpen(false)}
      title="Incoming links"
      subtitle="Download links sent by the browser extension or detected on your clipboard."
      width="max-w-2xl"
      footer={
        <button className="btn" onClick={() => setOpen(false)}>
          Close
        </button>
      }
    >
      {captures.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-12 text-center">
          <GlobeIcon className="h-8 w-8 text-slate-600" />
          <p className="mt-3 text-sm font-medium text-slate-300">No pending captures</p>
          <p className="mt-1 max-w-sm text-xs text-slate-500">
            Copied file links and extension captures appear here before they start downloading.
          </p>
        </div>
      ) : (
        <div className="space-y-2">
          {captures.map((offer) => (
            <div
              key={offer.id}
              className="rounded-xl border border-white/[0.07] bg-white/[0.02] px-4 py-3"
            >
              <div className="flex items-start gap-3">
                <span className="mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-lg border border-white/[0.06] bg-white/[0.03] text-slate-400">
                  {offer.kind === "download" ? (
                    <DownloadIcon className="h-4 w-4" />
                  ) : (
                    <GlobeIcon className="h-4 w-4" />
                  )}
                </span>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className={`chip ${KIND_STYLE[offer.kind] ?? KIND_STYLE.download}`}>
                      {offer.kind.toUpperCase()}
                    </span>
                    <span className="truncate text-xs font-medium text-slate-200">
                      {offer.filename ?? domainOf(offer.url)}
                    </span>
                  </div>
                  <p className="mt-1 truncate font-mono text-[10px] text-slate-500" title={offer.url}>
                    {offer.url}
                  </p>
                  {offer.page_url ? (
                    <p className="mt-0.5 truncate text-[10px] text-slate-600">
                      from {domainOf(offer.page_url)}
                    </p>
                  ) : (
                    <p className="mt-0.5 text-[10px] text-slate-600">
                      {offer.source === "clipboard" ? "from clipboard" : "from browser extension"}
                    </p>
                  )}
                </div>
                <div className="flex shrink-0 items-center gap-1.5">
                  <button
                    className="icon-btn"
                    title="Dismiss"
                    onClick={() => void dismiss(offer.id)}
                  >
                    <XIcon className="h-4 w-4" />
                  </button>
                  <button
                    className="btn btn-primary h-8 px-2.5 text-xs"
                    onClick={() => void accept(offer.id)}
                    disabled={offer.kind === "dash"}
                    title={offer.kind === "dash" ? "DASH downloads are not supported yet" : undefined}
                  >
                    {offer.kind === "dash"
                      ? "Unsupported"
                      : offer.kind === "hls"
                        ? "Grab video"
                        : "Download"}
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
      {captures.length > 0 ? (
        <div className="mt-3 flex items-center justify-between text-[10px] text-slate-500">
          <span className="flex items-center gap-1">
            <TrashIcon className="h-3 w-3" />
            Dismissed links can be captured again after copying them.
          </span>
          <button className="hover:text-slate-300" onClick={() => setOpen(false)}>
            {captures.length} pending
          </button>
        </div>
      ) : null}
    </Modal>
  );
}
