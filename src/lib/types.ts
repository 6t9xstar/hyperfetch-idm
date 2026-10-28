export type DownloadState =
  | "queued"
  | "probing"
  | "running"
  | "paused"
  | "completing"
  | "done"
  | "error"
  | "canceled";

export interface DownloadSnapshot {
  id: string;
  url: string;
  final_url: string | null;
  filename: string;
  dir: string;
  total_size: number;
  received: number;
  state: DownloadState;
  accept_ranges: boolean;
  connections: number;
  speed_limit: number;
  category: string | null;
  priority: number;
  error: string | null;
  sha256: string | null;
  created_at: number;
  updated_at: number;
  finished_at: number | null;
  speed: number;
  eta_secs: number | null;
  active_connections: number;
}

export interface ProgressEvent {
  id: string;
  received: number;
  total: number;
  speed: number;
  eta_secs: number | null;
  active_connections: number;
}

export type EngineEvent =
  | { type: "added"; download: DownloadSnapshot }
  | { type: "updated"; download: DownloadSnapshot }
  | { type: "progress"; progress: ProgressEvent }
  | { type: "removed"; id: string }
  | { type: "queue"; pending: number; active: number };

export interface DownloadSpec {
  url: string;
  dir?: string | null;
  filename?: string | null;
  category?: string | null;
  connections?: number | null;
  speed_limit: number;
  priority: number;
  headers: Record<string, string>;
  cookie?: string | null;
  user_agent?: string | null;
  proxy?: string | null;
  start_now: boolean;
}

export interface ProbeResult {
  final_url: string;
  filename: string;
  total_size: number;
  accept_ranges: boolean;
  etag: string | null;
  last_modified: string | null;
  content_type: string | null;
  content_disposition: string | null;
}

export interface Settings {
  download_dir: string;
  max_concurrent: number;
  connections_per_download: number;
  max_connections: number;
  batch_size: number;
  min_split_size: number;
  min_segmented_size: number;
  global_speed_limit: number;
  auto_resume: boolean;
  batch_retries: number;
  compute_sha256: boolean;
  durable_finish: boolean;
  categorize: boolean;
  category_folders: Record<string, string>;
  proxy: string | null;
  user_agent: string;
  clipboard_monitor: boolean;
  clipboard_auto_add: boolean;
  confirm_captures: boolean;
  bridge_port: number;
  theme: string;
}

export interface BridgeInfo {
  url: string;
  port: number;
  token: string;
}

export interface CaptureOffer {
  id: string;
  url: string;
  filename: string | null;
  kind: string;
  referer: string | null;
  cookie: string | null;
  user_agent: string | null;
  page_url: string | null;
  created_at: number;
  source?: "browser" | "clipboard";
  auto?: boolean;
}

export interface CaptureAction {
  action: "download" | "grabber" | string;
  id: string;
  message: string | null;
}

export interface ClipboardOffer {
  id: string;
  url: string;
  filename: string;
  total_size: number;
  content_type: string | null;
  kind: "download" | "hls" | string;
  auto: boolean;
  created_at: number;
}

export interface DiscoveredLink {
  url: string;
  depth: number;
  is_file: boolean;
}

export interface EngineStats {
  active: number;
  queued: number;
  speed: number;
}

export type Filter =
  | "all"
  | "downloading"
  | "queued"
  | "paused"
  | "finished"
  | "failed"
  | "video"
  | "audio"
  | "documents"
  | "archives"
  | "programs"
  | "images"
  | "other";
