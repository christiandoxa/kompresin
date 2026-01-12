# WASM Compressor (Trunk + Rust)

Static website for compressing:

- JPG / JPEG (lossy)
- PNG (lossless optimize, or "lossy palette" via quantization)
- PDF (text preserved; embedded images are recompressed)

> Note: PDF compression here **keeps text selectable** and only recompresses embedded images. If the PDF has no large
> images (or uses unsupported image formats), size savings may be small.

## Why pixo?

`pixo` is a pure-Rust PNG+JPEG encoder designed to be small and fast in WASM, including palette quantization for "lossy
PNG".  
Docs: https://docs.rs/pixo

## Run locally

1) Install target & trunk:

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk
```

2) Serve:

```bash
trunk serve --open
```

3) Build static:

```bash
trunk build --release
# output in dist/
```

## Deploy

Upload the `dist/` folder to static hosting (Netlify, Vercel static, GitHub Pages, Cloudflare Pages, etc.).

## Offline mode

All compression runs locally in the browser via WASM, with no external PDF libraries.
