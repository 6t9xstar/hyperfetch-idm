import { useStore } from "../store";
import { CheckIcon, XIcon } from "./Icons";

const STYLES: Record<string, string> = {
  info: "border-sky-400/25 bg-sky-950/90 text-sky-100",
  success: "border-accent-500/30 bg-emerald-950/90 text-emerald-100",
  error: "border-rose-500/30 bg-rose-950/90 text-rose-100",
};

export function Toasts() {
  const toasts = useStore((state) => state.toasts);
  const dismiss = useStore((state) => state.dismissToast);

  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-[70] flex w-80 flex-col gap-2">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={`hf-toast pointer-events-auto flex items-start gap-2 rounded-lg border px-3 py-2.5 text-xs shadow-xl backdrop-blur animate-slide-up ${STYLES[toast.kind]}`}
        >
          <span className="mt-0.5 shrink-0">
            {toast.kind === "success" ? (
              <CheckIcon className="h-4 w-4" />
            ) : (
              <span className="block h-4 w-4 text-center font-bold">!</span>
            )}
          </span>
          <p className="flex-1 leading-relaxed">{toast.message}</p>
          <button
            className="shrink-0 rounded p-0.5 opacity-60 transition hover:opacity-100"
            onClick={() => dismiss(toast.id)}
            aria-label="Dismiss notification"
          >
            <XIcon className="h-3.5 w-3.5" />
          </button>
        </div>
      ))}
    </div>
  );
}
