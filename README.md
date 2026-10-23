# nestbox.mmbits.com

The public Nestbox site: the landing page, the Proxmox install guide and the documentation.

This repository only holds the published artifact. The sources live in the private
`matthias1232/nestbox` repository:

- `site/` — landing page and install guide (HTML and CSS, no build step)
- `site/assets/guide/` — screenshots used by the guide
- `docs/` — documentation, rendered with MkDocs Material
- `scripts/site/build_site.sh` — assembles this artifact

```sh
./scripts/site/build_site.sh ../nestbox-site
```

The site is served by GitHub Pages from `main`. `CNAME` points `nestbox.mmbits.com` here.

Independent community project. Not affiliated with or endorsed by NVIDIA.
