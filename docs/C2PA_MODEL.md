# C2PA model

C2PA manifests contain claims, assertions, ingredients, and a signature bound to asset content. A Reader's successful parse is not itself a trust decision. Keep separate questions separate: whether credentials are present, whether the binding and signature validate, whether the signer chains to a configured trust root, and whether the derived asset has a meaningful relationship to its parent.

For an edit, the preceding asset is generally a `parentOf` ingredient. AI Media Provenance Checker records supported actions such as `c2pa.resized`, `c2pa.cropped`, or `c2pa.transcoded` in the new claim. A pixel re-encode is not merely repackaging. The SDK's Builder/Context APIs create and sign the new manifest; AI Media Provenance Checker does not invent a replacement cryptographic format.

A valid signature provides integrity and attribution of the signed assertions to a key. It does not prove the assertions are true, and a valid signature does not imply the signer is publicly trusted. Missing credentials do not establish that an image is original, human-created, or unmodified.
