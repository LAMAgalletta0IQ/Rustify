/**
 * Cover-art encoding for playlist uploads.
 *
 * `PUT /playlists/{id}/images` accepts **base64 JPEG only**, capped at 256 KB
 * of base64 (~192 KB of image). A phone photo is an order of magnitude past
 * that and typically a PNG or HEIC besides, so a picked file has to be
 * re-encoded before it can be sent. Doing it here, in the webview, means the
 * Rust side needs no image codec at all — it just forwards bytes.
 */

/** Spotify's documented limit, in bytes of base64. Mirrors
 * `library::MAX_PLAYLIST_IMAGE_BASE64`. */
export const MAX_COVER_BASE64 = 256 * 1024;

/** Spotify shows covers at 640px at most; anything larger is spent bandwidth. */
const MAX_EDGE = 640;

/** Quality ladder. Each step is tried in turn until the result fits, rather
 * than guessing once — JPEG size at a given quality varies enormously with
 * image content, so a fixed quality either wastes headroom or overshoots. */
const QUALITY_STEPS = [0.9, 0.8, 0.7, 0.6, 0.5, 0.4];

function loadImage(file: File): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file);
    const image = new Image();
    image.onload = () => {
      URL.revokeObjectURL(url);
      resolve(image);
    };
    image.onerror = () => {
      URL.revokeObjectURL(url);
      reject(new Error("That file could not be read as an image."));
    };
    image.src = url;
  });
}

/**
 * Centre-crops to a square, downscales to at most 640px, and encodes as JPEG
 * at the highest quality that fits under Spotify's cap.
 *
 * Returns bare base64 (no `data:` prefix). Throws when even the lowest quality
 * is still too large, which in practice only happens for images that are noise
 * at full frame.
 */
export async function encodeCoverJpeg(file: File): Promise<string> {
  const image = await loadImage(file);
  // Square, because Spotify renders every playlist cover as one — cropping
  // here means the user sees the same framing the app will.
  const edge = Math.min(image.naturalWidth, image.naturalHeight);
  const size = Math.min(MAX_EDGE, edge);
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = size;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("This system could not prepare the image.");
  context.drawImage(
    image,
    (image.naturalWidth - edge) / 2,
    (image.naturalHeight - edge) / 2,
    edge,
    edge,
    0,
    0,
    size,
    size,
  );

  for (const quality of QUALITY_STEPS) {
    const encoded = canvas.toDataURL("image/jpeg", quality).split(",")[1] ?? "";
    if (encoded && encoded.length <= MAX_COVER_BASE64) return encoded;
  }
  throw new Error(
    "That image is too detailed to compress under Spotify's 256 KB cover limit. Try a simpler or smaller one.",
  );
}
