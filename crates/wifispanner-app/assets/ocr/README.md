# Bundled text-reading models

`text-detection.rten` and `text-recognition.rten` are the pre-trained models of the Ocrs OCR engine
(https://github.com/robertknight/ocrs-models, downloaded from https://huggingface.co/robertknight/ocrs
as `text-detection-ssfbcj81.rten` and `text-rec-checkpoint-s52qdbqt.rten`), used unmodified (only
renamed). They were trained on the HierText dataset by Google Research and are licensed under
Creative Commons Attribution-ShareAlike 4.0 (full text in `LICENSE.txt`, as published at
https://creativecommons.org/licenses/by-sa/4.0/legalcode.txt).

Ocrs models (https://github.com/robertknight/ocrs-models), trained on the HierText dataset by Google
Research. Licensed under CC BY-SA 4.0.

The licence covers these two data files only, not WiFi Spanner's code. They are used to read the
dimensions printed on a floor plan (see `src/autoscale.rs`).
