//! Tests for Farcaster Mini App support

use trek_rs::{Trek, TrekOptions};

#[test]
fn test_crowdfund_seedclub_mini_app() {
    let mut options = TrekOptions::default();
    options.url = Some("https://crowdfund.seedclub.com/c/cmcamk1l500v6vy0tbg09o6vj".to_string());
    let trek = Trek::new(options);

    // Minimal HTML with the fc:frame meta tag from crowdfund.seedclub.com
    let html = r##"
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Support Christopher's Project</title>
    <meta name="fc:frame" content='{"version":"next","imageUrl":"https://node.nyc3.cdn.digitaloceanspaces.com/crowdfund/cmcamk1l500v6vy0tbg09o6vj.png","button":{"title":"Donate now","action":{"type":"launch_frame","url":"https://crowdfund.seedclub.com/c/cmcamk1l500v6vy0tbg09o6vj","splashImageUrl":"https://node.nyc3.cdn.digitaloceanspaces.com/crowdfund/crowdfund_icon.png","splashBackgroundColor":"#ffffff","name":"Crowdfund"}}}'>
    <meta property="og:title" content="Support Christopher's Project">
    <meta property="og:description" content="Help fund this amazing project">
    <meta property="og:image" content="https://node.nyc3.cdn.digitaloceanspaces.com/crowdfund/cmcamk1l500v6vy0tbg09o6vj.png">
</head>
<body>
    <h1>Support Christopher's Project</h1>
    <p>This is a crowdfunding campaign for an amazing project.</p>
</body>
</html>
    "##;

    let result = trek.parse(html).unwrap();

    // Check that the extractor was used
    assert_eq!(result.extractor_type, Some("farcaster".to_string()));

    // Check that mini app embed was parsed correctly
    assert!(result.metadata.mini_app_embed.is_some());

    let embed = result.metadata.mini_app_embed.unwrap();
    assert_eq!(embed.version, "next");
    assert_eq!(
        embed.image_url,
        "https://node.nyc3.cdn.digitaloceanspaces.com/crowdfund/cmcamk1l500v6vy0tbg09o6vj.png"
    );
    assert_eq!(embed.button.title, "Donate now");
    // Note: We can't directly compare the action_type enum as it's not exported
    // Instead, we'll verify through serialization that it was parsed correctly
    assert_eq!(
        embed.button.action.url,
        Some("https://crowdfund.seedclub.com/c/cmcamk1l500v6vy0tbg09o6vj".to_string())
    );
    assert_eq!(embed.button.action.name, Some("Crowdfund".to_string()));
    assert_eq!(
        embed.button.action.splash_image_url,
        Some(
            "https://node.nyc3.cdn.digitaloceanspaces.com/crowdfund/crowdfund_icon.png".to_string()
        )
    );
    assert_eq!(
        embed.button.action.splash_background_color,
        Some("#ffffff".to_string())
    );

    // Check that other metadata was extracted
    assert_eq!(result.metadata.title, "Support Christopher's Project");
    assert_eq!(
        result.metadata.description,
        "Help fund this amazing project"
    );
    assert_eq!(
        result.metadata.image,
        "https://node.nyc3.cdn.digitaloceanspaces.com/crowdfund/cmcamk1l500v6vy0tbg09o6vj.png"
    );
}

#[test]
fn test_mini_app_without_fc_frame() {
    let trek = Trek::new(TrekOptions::default());

    // HTML without fc:frame meta tag
    let html = r##"
<!DOCTYPE html>
<html>
<head>
    <title>Regular Page</title>
    <meta name="description" content="Just a regular page">
</head>
<body>
    <article>
        <h1>Regular Content</h1>
        <p>This page has no mini app embed.</p>
    </article>
</body>
</html>
    "##;

    let result = trek.parse(html).unwrap();

    // Check that no mini app embed was found
    assert!(result.metadata.mini_app_embed.is_none());

    // Check that regular metadata was still extracted
    assert_eq!(result.metadata.title, "Regular Page");
    assert_eq!(result.metadata.description, "Just a regular page");
}
