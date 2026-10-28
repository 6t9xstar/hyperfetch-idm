use axum::routing::get;
use axum::Router;
use hyperfetch_core::grabber::hls::download_hls;

const MASTER: &str = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1200000\nmedia.m3u8\n";

const MEDIA: &str = "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:4\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:4.0,\nseg0.m4s\n#EXTINF:4.0,\nseg1.m4s\n#EXT-X-ENDLIST\n";

#[tokio::test]
async fn downloads_fragmented_mp4_with_init_section() {
    let app = Router::new()
        .route("/master.m3u8", get(|| async { MASTER }))
        .route("/media.m3u8", get(|| async { MEDIA }))
        .route("/init.mp4", get(|| async { vec![1u8, 2, 3] }))
        .route("/seg0.m4s", get(|| async { vec![10u8, 11] }))
        .route("/seg1.m4s", get(|| async { vec![20u8, 21, 22] }));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let url = url::Url::parse(&format!("http://{addr}/master.m3u8")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("video.ts");

    let summary = download_hls(&client, &url, &output, 2, None).await.unwrap();

    assert_eq!(summary.segments, 2);
    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(bytes, vec![1, 2, 3, 10, 11, 20, 21, 22]);
    assert_eq!(summary.bytes, bytes.len() as u64);
}
