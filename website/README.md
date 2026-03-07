# LUPA Website

Documentation website for LUPA, isolated from app/runtime logic.

## Selected Stack (Impact-First)

- **Astro**: static-first performance, excellent SEO, clean content architecture.
- **TypeScript**: safer component and content code.
- **Tailwind CSS**: fast visual iteration for premium UI.
- **Motion One**: lightweight animations with good performance.
- **MDX**: docs pages and rich technical content blocks.

## Why This Stack

- Fast first paint and low JS by default.
- Easy to ship high-end visual identity without hurting performance.
- Clear separation between:
  - product/app code (`crates/`)
  - public site (`website/`)

## Initial Site Goals

1. Hero section with strong value proposition and direct download CTA.
2. Benchmarks and architecture highlights (`p95`, offline-first, privacy-first).
3. Feature overview and screenshots.
4. Release links (`.exe` + `.msi`) and checksum visibility.
5. Contributing and roadmap links.

## Planned Pages

- `/` Home 
- `/docs` public docs entrypoint
- `/benchmarks`
- `/roadmap`
- `/download`

## Next Step

Initialize project dependencies:

```bash
cd website
npm install
npm run dev
```
