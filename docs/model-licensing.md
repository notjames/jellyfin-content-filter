# Model and third-party licensing

The project package does not include machine-learning model weights or training media. The MIT license covers only project-authored code and documentation; it does not grant rights to Jellyfin, media, model weights, datasets, codecs, or other dependencies.

Before distributing or enabling a model:

1. Record its exact name, version, source URL, checksum, license, and required attribution.
2. Confirm the license permits the intended use, modification, and redistribution.
3. Review the training-data statement and acceptable-use restrictions.
4. Store license and notice text beside the model.
5. Verify the checksum before loading it and pin updates explicitly.

Do not download models at service startup without administrator consent. The example configuration keeps models disabled. A package that redistributes weights must add a model-specific notice and source information; describing weights as “open source” is not a substitute for checking their actual terms.

Generated labels can be inaccurate and biased. Keep a human-editable policy path, expose provenance where practical, and avoid using labels for decisions unrelated to playback preferences.
