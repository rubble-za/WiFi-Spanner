# Carbon icons

The icons in `svg/` are from the IBM Carbon Design System icon set
(https://github.com/carbon-design-system/carbon, `packages/icons/src/svg/32`), Copyright IBM Corp.,
licensed under the Apache License, Version 2.0 (full text in `LICENSE.txt`, copied unchanged from the
repository's `LICENSE`). The icon package declares "license": "Apache-2.0" and the repository has no
NOTICE file.

**Changes made:** the `svg/` files are unmodified. The files in `png/` were produced from them by
rasterising each to 96 x 96 pixels and recolouring the drawing white on a transparent background, so the
app can tint them at run time. The command is:

    magick -background none -density 600 in.svg -resize 96x96 -channel RGB -evaluate set 100% +channel out.png
