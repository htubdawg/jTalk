The bundled dictionary is the unmodified `cmudict.dict` from CMUSphinx at
[74790861](https://github.com/cmusphinx/cmudict/tree/74790861f652b15e4ac49015a90074ad62a27690),
compressed with `gzip -n -9`. See [CMUDICT-LICENSE](CMUDICT-LICENSE) for its
redistribution terms.

The spelling fallback uses the American English character-to-phoneme model
by Peter Reid at revision `a5631b285d18d59483c32c0c3379cb9fac924f4b`,
provided by the Rust crate [loqui-g2p](https://crates.io/crates/loqui-g2p).
The model is licensed under Apache-2.0. The Windows package includes its
license and the crate's MIT license and notices.
[Model source](https://huggingface.co/PeterReid/graphemes_to_phonemes_en_us/tree/a5631b285d18d59483c32c0c3379cb9fac924f4b)
