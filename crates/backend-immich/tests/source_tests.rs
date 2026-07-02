use futures::StreamExt;
use rs_immicher_oxide_backend_immich::source::ImmichSource;
use rs_immicher_oxide_backend_immich::ImmichConfig;
use rs_immicher_oxide_core::source::Source;
use rs_immicher_oxide_core::types::MediaCodec;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn make_asset_item(id: &str, fname: &str, mime: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "type": "IMAGE",
        "originalFileName": fname,
        "originalMimeType": mime,
        "fileCreatedAt": "2024-01-01T00:00:00.000Z",
        "fileModifiedAt": "2024-01-01T00:00:00.000Z",
        "isFavorite": false,
        "isArchived": false,
        "checksum": null,
        "exifInfo": null
    })
}

fn make_search_response(
    items: Vec<serde_json::Value>,
    next_page: Option<&str>,
    total: i32,
) -> serde_json::Value {
    serde_json::json!({
        "assets": {
            "items": items,
            "nextPage": next_page,
            "total": total,
            "facets": []
        },
        "albums": { "items": [], "nextPage": null, "total": 0 }
    })
}

#[tokio::test]
async fn discover_yields_first_page_without_collecting_all_pages() {
    let mock = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/search/metadata"))
        .and(body_partial_json(serde_json::json!({ "page": 1 })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(make_search_response(
                vec![make_asset_item("a1", "a1.jpg", "image/jpeg")],
                Some("2"),
                2,
            )),
        )
        .mount(&mock)
        .await;

    let source = ImmichSource::new(
        ImmichConfig::new(mock.uri(), "test-api-key"),
        MediaCodec::Jxl,
    )
    .with_images_only();

    let mut stream = source
        .discover()
        .await
        .expect("discover should construct a lazy stream");
    let first = stream
        .next()
        .await
        .expect("first page should yield an asset")
        .expect("first page item should map successfully");

    assert_eq!(first.id, "a1");
}
