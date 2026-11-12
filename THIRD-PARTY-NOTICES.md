# Third-party notices

Endora Core is licensed under the Apache License 2.0 (`LICENSE`, `NOTICE`). This file lists the
third-party software that Core's image carries, under what licence, and where the source of the
copyleft parts can be had. It covers the final (default) stage of the Dockerfile, the node image.
It does not cover the Dockerfile's other targets (`sound`, `senders`, `airplay2`, `worksite`,
`bench`).

This file is generated from the image itself, by `scripts/gen-third-party-notices.sh`. The licence
texts of the Rust crates are in the image, in `/usr/share/doc/endora-core/licenses/`, and the
Debian packages' own notices in `/usr/share/doc/<package>/copyright`.

## 1. What the image contains

Endora's own programs and scripts (Apache-2.0):

- `/usr/local/bin/endora-core` (link to endora-node)
- `/usr/local/bin/endora-hq` (file)
- `/usr/local/bin/endora-node` (file)
- `/usr/local/bin/endora-runner` (file)
- `/usr/local/bin/endora-worker` (file)
- `/usr/local/lib/endora/homelab-docker-proxy.py` (file)

Nothing in the image was patched: the Debian packages are installed as Debian builds them, and the
crates are built from the crates.io sources `Cargo.lock` names.

**glibc.** Endora's programs use the GNU C Library (glibc, LGPL-2.1 or later) at run time, as the
shared libraries `libc.so.6` and `libm.so.6`, and `libgcc_s.so.1` (GCC, with the GCC Runtime
Library Exception). They are not modified; the licence texts are in
`/usr/share/doc/libc6/copyright` and `/usr/share/common-licenses/`.

## 2. Rust crates

152 third-party crates linked into Endora's programs (Endora's own workspace crates, all
Apache-2.0, are not listed). The licence column is the crate's declared SPDX expression; the
copyright column is the first copyright line of the crate's own licence files. The licence texts
themselves are in the image, in `/usr/share/doc/endora-core/licenses/<crate>-<version>/`.

| Crate | Version | Licence | Copyright |
|---|---|---|---|
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | Copyright (C) Jonas Schievink |
| aead | 0.5.2 | MIT OR Apache-2.0 | Copyright (c) 2019 The RustCrypto Project Developers |
| arc-swap | 1.9.2 | MIT OR Apache-2.0 | Copyright (c) 2017 arc-swap developers |
| argon2 | 0.5.3 | MIT OR Apache-2.0 | Copyright (c) 2021-2024 The RustCrypto Project Developers |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | Copyright (c) 2016 Alex Crichton |
| aws-lc-rs | 1.17.3 | ISC AND (Apache-2.0 OR ISC) |  |
| aws-lc-sys | 0.43.0 | ISC AND (Apache-2.0 OR ISC) AND Apache-2.0 AND MIT AND BSD-3-Clause AND (Apache-2.0 OR ISC OR MIT) AND (Apache-2.0 OR ISC OR MIT-0) | Copyright (c) 2014-2024 Google Inc. |
| axum | 0.8.9 | MIT | Copyright (c) 2019 axum Contributors |
| axum-core | 0.5.6 | MIT | Copyright (c) 2019–2025 axum Contributors |
| axum-server | 0.7.3 | MIT | Copyright 2021 Axum Server Contributors |
| base64 | 0.22.1 | MIT OR Apache-2.0 | Copyright (c) 2015 Alice Maz |
| base64 | 0.23.1 | MIT OR Apache-2.0 | Copyright (c) 2025 Alice Maz, Marshall Pierce |
| base64ct | 1.8.3 | Apache-2.0 OR MIT | Copyright (c) 2014 Steve "Sc00bz" Thomas (steve at tobtu dot com) |
| bitflags | 2.13.1 | MIT OR Apache-2.0 | Copyright (c) 2014 The Rust Project Developers |
| blake2 | 0.10.6 | MIT OR Apache-2.0 | Copyright (c) 2015-2016 The blake2-rfc Developers, Cesar Barros |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | Copyright (c) 2018-2019 The RustCrypto Project Developers |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 | Copyright (c) 2018-2025 The RustCrypto Project Developers |
| byteorder | 1.5.0 | Unlicense OR MIT | Copyright (c) 2015 Andrew Gallant |
| bytes | 1.12.1 | MIT | Copyright (c) 2018 Carl Lerche |
| cfg-if | 1.0.4 | MIT OR Apache-2.0 | Copyright (c) 2014 Alex Crichton |
| chacha20 | 0.10.1 | MIT OR Apache-2.0 | Copyright (c) 2019-2026 The RustCrypto Project Developers |
| chacha20 | 0.9.1 | Apache-2.0 OR MIT | Copyright (c) 2019-2023 The RustCrypto Project Developers |
| chacha20poly1305 | 0.10.1 | Apache-2.0 OR MIT | Copyright (c) 2019 The RustCrypto Project Developers |
| chrono | 0.4.45 | MIT OR Apache-2.0 | Copyright (c) 2014, Kang Seonghoon. |
| chrono-tz | 0.10.4 | MIT OR Apache-2.0 | Copyright (c) 2016-2024 Benjamin Sago & the chronotope maintainers |
| cipher | 0.4.4 | MIT OR Apache-2.0 | Copyright (c) 2016-2020 RustCrypto Developers |
| const-oid | 0.10.2 | Apache-2.0 OR MIT | Copyright (c) 2020-2026 The RustCrypto Project Developers |
| cozy-chess | 0.3.4 | MIT | Copyright (c) analog-hors NEEDS-OWNER-REVIEW |
| cozy-chess-types | 0.2.2 | MIT | Copyright (c) analog-hors NEEDS-OWNER-REVIEW |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | Copyright (c) 2020-2025 The RustCrypto Project Developers |
| cpufeatures | 0.3.0 | MIT OR Apache-2.0 | Copyright (c) 2020-2025 The RustCrypto Project Developers |
| crc32fast | 1.5.0 | MIT OR Apache-2.0 | Copyright (c) 2018 Sam Rijs, Alex Crichton and contributors |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | Copyright (c) 2021 RustCrypto Developers |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 | Copyright (c) 2021-2026 RustCrypto Developers |
| data-encoding | 2.11.0 | MIT | Copyright (c) 2015-2020 Julien Cretin |
| deranged | 0.5.8 | MIT OR Apache-2.0 | Copyright 2024 Jacob Pratt et al. |
| digest | 0.10.7 | MIT OR Apache-2.0 | Copyright (c) 2017 Artyom Pavlov |
| digest | 0.11.3 | MIT OR Apache-2.0 | Copyright (c) 2017-2025 RustCrypto Developers |
| equivalent | 1.0.2 | Apache-2.0 OR MIT | Copyright (c) 2016--2023 |
| fallible-iterator | 0.3.0 | MIT/Apache-2.0 | Copyright (c) 2015 The rust-openssl-verify Developers |
| fallible-streaming-iterator | 0.1.9 | MIT/Apache-2.0 | Copyright (c) 2016 The fallible-streaming-iterator Developers |
| flate2 | 1.1.9 | MIT OR Apache-2.0 | Copyright (c) 2014-2026 Alex Crichton |
| fnv | 1.0.7 | Apache-2.0 / MIT | Copyright (c) 2017 Contributors |
| foldhash | 0.2.0 | Zlib | Copyright (c) 2024 Orson Peters |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | Copyright (c) 2013-2016 The rust-url developers |
| fs-err | 3.3.1 | MIT OR Apache-2.0 |  |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | Copyright (c) 2016 Alex Crichton |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | Copyright (c) 2016 Alex Crichton |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | Copyright (c) 2016 Alex Crichton |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 | Copyright (c) 2016 Alex Crichton |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | Copyright (c) 2016 Alex Crichton |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | Copyright (c) 2016 Alex Crichton |
| generic-array | 0.14.7 | MIT | Copyright (c) 2015 Bartłomiej Kamiński |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | Copyright (c) 2018-2024 The rust-random Project Developers |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | Copyright (c) 2018-2026 The rust-random Project Developers |
| h2 | 0.4.15 | MIT | Copyright (c) 2017 h2 authors |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | Copyright (c) 2016 Amanieu d'Antras |
| hashlink | 0.12.2 | MIT OR Apache-2.0 |  |
| http | 1.4.2 | MIT OR Apache-2.0 | Copyright 2017 http-rs authors |
| http-body | 1.1.0 | MIT | Copyright (c) 2019-2026 Sean McArthur & Hyper Contributors |
| http-body-util | 0.1.5 | MIT | Copyright (c) 2019-2026 Sean McArthur & Hyper Contributors |
| httparse | 1.10.1 | MIT OR Apache-2.0 | Copyright (c) 2015-2025 Sean McArthur |
| httpdate | 1.0.3 | MIT OR Apache-2.0 | Copyright (c) 2016 Pyfisch |
| hybrid-array | 0.4.13 | MIT OR Apache-2.0 | Copyright (c) 2022-2026 The RustCrypto Project Developers |
| hyper | 1.10.1 | MIT | Copyright (c) 2014-2026 Sean McArthur |
| hyper-util | 0.1.20 | MIT | Copyright (c) 2023-2025 Sean McArthur |
| indexmap | 2.14.0 | Apache-2.0 OR MIT | Copyright (c) 2016--2017 |
| inout | 0.1.4 | MIT OR Apache-2.0 | Copyright (c) 2022 The RustCrypto Project Developers |
| itoa | 1.0.18 | MIT OR Apache-2.0 |  |
| libc | 0.2.186 | MIT OR Apache-2.0 | Copyright (c) The Rust Project Developers |
| libsqlite3-sys | 0.38.2 | MIT | Copyright (c) 2014 The rusqlite developers |
| log | 0.4.33 | MIT OR Apache-2.0 | Copyright (c) 2014 The Rust Project Developers |
| matchit | 0.8.4 | MIT AND BSD-3-Clause | Copyright (c) 2022 Ibraheem Ahmed |
| memchr | 2.8.3 | Unlicense OR MIT | Copyright (c) 2015 Andrew Gallant |
| mime | 0.3.17 | MIT OR Apache-2.0 | Copyright (c) 2014 Sean McArthur |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | Copyright 2013-2014 RAD Game Tools and Valve Software |
| mio | 1.2.2 | MIT | Copyright (c) 2014 Carl Lerche and other MIO contributors |
| num-conv | 0.1.0 | MIT OR Apache-2.0 | Copyright 2023 Jacob Pratt |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | Copyright (c) 2014 The Rust Project Developers |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |  |
| opaque-debug | 0.3.1 | MIT OR Apache-2.0 | Copyright (c) 2018-2024 The RustCrypto Project Developers |
| openssl-probe | 0.2.1 | MIT OR Apache-2.0 | Copyright (c) 2014 Alex Crichton |
| password-hash | 0.5.0 | MIT OR Apache-2.0 | Copyright (c) 2020-2023 RustCrypto Developers |
| pem | 3.0.6 | MIT | Copyright (c) 2016 Jonathan Creekmore |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | Copyright (c) 2013-2025 The rust-url developers |
| phf | 0.12.1 | MIT | Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi |
| phf_shared | 0.12.1 | MIT | Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT |  |
| poly1305 | 0.8.0 | Apache-2.0 OR MIT | Copyright (c) 2015-2019 RustCrypto Developers |
| powerfmt | 0.2.0 | MIT OR Apache-2.0 | Copyright 2023 Jacob Pratt et al. |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |  |
| qrcode | 0.14.1 | MIT OR Apache-2.0 | Copyright (c) 2016 kennytm |
| quote | 1.0.47 | MIT OR Apache-2.0 |  |
| rand | 0.10.2 | MIT OR Apache-2.0 | Copyright 2018 Developers of the Rand project |
| rand_core | 0.10.1 | MIT OR Apache-2.0 | Copyright (c) 2018-2026 The Rand Project Developers |
| rand_core | 0.6.4 | MIT OR Apache-2.0 | Copyright 2018 Developers of the Rand project |
| rcgen | 0.13.2 | MIT OR Apache-2.0 | Copyright (c) 2019-2022 est31 and contributors |
| ring | 0.17.14 | Apache-2.0 AND ISC | Copyright (c) 2009 The Go Authors. All rights reserved. |
| rpassword | 7.5.4 | Apache-2.0 |  |
| rtoolbox | 0.0.6 | Apache-2.0 |  |
| rusqlite | 0.40.2 | MIT | Copyright (c) 2014 The rusqlite developers |
| rustls | 0.23.43 | Apache-2.0 OR ISC OR MIT | Copyright (c) 2016, Joseph Birr-Pixton |
| rustls-native-certs | 0.8.4 | Apache-2.0 OR ISC OR MIT | Copyright (c) 2016, Joseph Birr-Pixton |
| rustls-pemfile | 2.2.0 | Apache-2.0 OR ISC OR MIT | Copyright (c) 2016, Joseph Birr-Pixton |
| rustls-pki-types | 1.15.0 | MIT OR Apache-2.0 | Copyright 2023 Dirkjan Ochtman |
| rustls-webpki | 0.103.13 | ISC | Copyright 2015 Brian Smith. |
| rustversion | 1.0.23 | MIT OR Apache-2.0 |  |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 |  |
| serde | 1.0.229 | MIT OR Apache-2.0 |  |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |  |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 |  |
| serde_json | 1.0.150 | MIT OR Apache-2.0 |  |
| serde_path_to_error | 0.1.20 | MIT OR Apache-2.0 |  |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 | Copyright (c) 2016 Anthony Ramine |
| sha1 | 0.11.0 | MIT OR Apache-2.0 | Copyright (c) 2016-2026 The RustCrypto Project Developers |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | Copyright (c) 2006-2009 Graydon Hoare |
| simd-adler32 | 0.3.10 | MIT | Copyright (c) [2021] [Marvin Countryman] |
| siphasher | 1.0.4 | MIT OR Apache-2.0 | Copyright 2012-2016 The Rust Project Developers. |
| slab | 0.4.12 | MIT | Copyright (c) 2019 Carl Lerche |
| smallvec | 1.15.2 | MIT OR Apache-2.0 | Copyright (c) 2018 The Servo Project Developers |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | Copyright (c) 2014 Alex Crichton |
| socks | 0.3.4 | MIT/Apache-2.0 | Copyright (c) 2015 The rust-socks Developers |
| subtle | 2.6.1 | BSD-3-Clause | Copyright (c) 2016-2017 Isis Agora Lovecruft, Henry de Valence. All rights reserved. |
| syn | 2.0.119 | MIT OR Apache-2.0 |  |
| syn | 3.0.0 | MIT OR Apache-2.0 |  |
| sync_wrapper | 1.0.2 | Apache-2.0 |  |
| thiserror | 2.0.19 | MIT OR Apache-2.0 |  |
| thiserror-impl | 2.0.19 | MIT OR Apache-2.0 |  |
| time | 0.3.45 | MIT OR Apache-2.0 | Copyright (c) Jacob Pratt et al. |
| time-core | 0.1.7 | MIT OR Apache-2.0 | Copyright (c) Jacob Pratt et al. |
| tokio | 1.53.0 | MIT | Copyright (c) Tokio Contributors |
| tokio-macros | 2.7.1 | MIT | Copyright (c) 2019 Yoshua Wuyts |
| tokio-rustls | 0.26.4 | MIT OR Apache-2.0 | Copyright 2017 quininer kel |
| tokio-util | 0.7.18 | MIT | Copyright (c) Tokio Contributors |
| tower | 0.5.3 | MIT | Copyright (c) 2019 Tower Contributors |
| tower-layer | 0.3.3 | MIT | Copyright (c) 2019 Tower Contributors |
| tower-service | 0.3.3 | MIT | Copyright (c) 2019 Tower Contributors |
| tracing | 0.1.44 | MIT | Copyright (c) 2019 Tokio Contributors |
| tracing-core | 0.1.36 | MIT | Copyright (c) 2019 Tokio Contributors |
| tungstenite | 0.30.0 | MIT OR Apache-2.0 | Copyright (c) 2017 Alexey Galakhov |
| typenum | 1.20.1 | MIT OR Apache-2.0 | Copyright 2014 Paho Lurie-Gregg |
| unicode-ident | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 | Copyright © 1991-2023 Unicode, Inc. |
| universal-hash | 0.5.1 | MIT OR Apache-2.0 | Copyright (c) 2019-2020 RustCrypto Developers |
| untrusted | 0.9.0 | ISC |  |
| ureq | 3.4.2 | MIT OR Apache-2.0 | Copyright (c) 2019 Martin Algesten |
| ureq-proto | 0.6.4 | MIT OR Apache-2.0 | Copyright 2022 Martin Algesten |
| utf8-zero | 0.8.1 | MIT OR Apache-2.0 |  |
| uuid | 1.26.1 | Apache-2.0 OR MIT | Copyright (c) 2014 The Rust Project Developers |
| webpki-roots | 1.0.9 | CDLA-Permissive-2.0 |  |
| yasna | 0.5.2 | MIT OR Apache-2.0 | Copyright (c) Masaki Hara NEEDS-OWNER-REVIEW |
| zeroize | 1.9.0 | Apache-2.0 OR MIT | Copyright (c) 2018-2026 The RustCrypto Project Developers |
| zmij | 1.0.23 | MIT |  |

## 3. Debian packages

The 495 packages of the image, with the licences their own `copyright` files name (the
`License:` lines of a machine-readable file; a package that bundles other code names several).
A package marked free-form has a copyright file with no `License:` lines: it is still scanned for
copyleft licences in section 5. The authoritative text of each is in the image:
`/usr/share/doc/<package>/copyright`, and the full texts of the common licences in
`/usr/share/common-licenses/`.

| Package | Version | Source package | Licences named |
|---|---|---|---|
| adduser | 3.134 | adduser | GPL-2+ |
| apt | 2.6.2 | apt | BSD-3-clause;Expat;GPL-2;GPL-2+ |
| base-files | 12.4+deb12u15 | base-files | (free-form copyright file) |
| base-passwd | 3.6.1 | base-passwd | GPL-2;public-domain |
| bash | 5.2.15-2+b13 | bash | BSD-4-clause-UC;BSD-4-clause-UC and MIT-like;GFDL-NIV-1.3;GPL-2+;GPL-3+;GPL-3+ with Bison exception;Latex2e;MIT-like;permissive |
| bsdutils | 1:2.38.1-5+deb12u3 | util-linux | BSD-3-clause;BSD-4-clause;BSLA;GPL-2;GPL-2+;GPL-3+;LGPL;LGPL-2+;LGPL-2.1+;LGPL-3+;MIT;public-domain |
| ca-certificates | 20250419~deb12u2 | ca-certificates | GPL-2+;MPL-2.0 |
| coreutils | 9.1-1 | coreutils | BSD-4-clause-UC;FSFULLR;GFDL-NIV-1.3;GPL-3+;GPL-3+ and BSD-4-clause-UC;GPL-3+ and ISC;ISC |
| dash | 0.5.12-2 | dash | BSD-3-Clause;BSD-3-clause;GPL-2+;public-domain |
| debconf | 1.5.82 | debconf | BSD-2-clause |
| debian-archive-keyring | 2023.3+deb12u2 | debian-archive-keyring | (free-form copyright file) |
| debianutils | 5.7-0.5~deb12u1 | debianutils | GPL-2+;SMAIL-GPL;public-domain |
| diffutils | 1:3.8-4 | diffutils | FSFAP;FSFULLR;GFDL-NIV-1.3;GPL-2+;GPL-3+;GPL-3+ and FSFULLR;GPL-3+ with autoconf exception;GPL-3+ with texinfo exception;LGPL-2.0+;LGPL-2.1+;LGPL-3.0+;LGPL-3.0+ or GPL-2+;X11;public-domain |
| dpkg | 1.21.23 | dpkg | BSD-2-clause;GPL-2;GPL-2+;public-domain-s-s-d |
| e2fsprogs | 1.47.0-2+b2 | e2fsprogs | Apache-2;BSD-3-Clause;GPL or MIT-US-export;GPL-2;GPL-2+ with Texinfo exception;ISC;Kazlib;LGPL-2;Latex2e |
| eslint | 6.4.0~dfsg+~6.1.9-7 | eslint | CC-BY-SA-2.0~UK;Expat;GPL-3+ |
| findutils | 4.9.0-4 | findutils | BSD-3-clause;BSD-3-clause and/or GPL-3+;FSFAP;FSFULLR;GFDL-NIV-1.3+;GPL with automake exception;GPL-2+;GPL-2+ with Autoconf-data exception;GPL-3+;GPL-3+ with Autoconf-data exception;GPL-3+ with Bison-2.2 exception;ISC;ISC and/or LGPL-2.1+;LGPL-2+;LGPL-2.1+;LGPL-3;LGPL-3+;X11;public-domain |
| gcc-12-base | 12.2.0-14+deb12u1 | gcc-12 | (free-form copyright file) |
| git | 1:2.39.5-0+deb12u3 | git | Apache-2.0;BSD-3-clause;Boost;EDL-1.0;Expat;GPL-1+ or Artistic-1;GPL-2;GPL-2+;ISC;LGPL-2+;LGPL-2.1+;Zlib;dlmalloc;mingw-runtime |
| git-man | 1:2.39.5-0+deb12u3 | git | Apache-2.0;BSD-3-clause;Boost;EDL-1.0;Expat;GPL-1+ or Artistic-1;GPL-2;GPL-2+;ISC;LGPL-2+;LGPL-2.1+;Zlib;dlmalloc;mingw-runtime |
| gpgv | 2.2.40-1.1+deb12u2 | gnupg2 | BSD-3-clause;CC0-1.0;Expat;GPL-3+;GPL-3+ or BSD-3-clause;LGPL-2.1+;LGPL-3+;RFC-Reference;TinySCHEME;permissive |
| grep | 3.8-5 | grep | GPL-3+ |
| gyp | 0.1+20220404git9ecf45e-2 | gyp | BSD-3-clause~Google |
| gzip | 1.12-1 | gzip | FSF-manpages;GFDL-1.3+-no-invariant;GPL-3+ |
| handlebars | 3:4.7.7+~4.1.0-1 | node-handlebars | Apache-2.0;Expat;LGPL-3 |
| hostname | 3.23+nmu1 | hostname | GPL-2 |
| init-system-helpers | 1.65.2+deb12u1 | init-system-helpers | BSD-3-clause;GPL-2+ |
| libacl1 | 2.3.1-3 | acl | GPL-2+;LGPL-2+ |
| libapt-pkg6.0 | 2.6.2 | apt | BSD-3-clause;Expat;GPL-2;GPL-2+ |
| libattr1 | 1:2.5.1-4 | attr | GPL-2+;LGPL-2+ |
| libaudit-common | 1:3.0.9-1 | audit | GPL-2;LGPL-2.1 |
| libaudit1 | 1:3.0.9-1 | audit | GPL-2;LGPL-2.1 |
| libblkid1 | 2.38.1-5+deb12u3 | util-linux | BSD-3-clause;BSD-4-clause;BSLA;GPL-2;GPL-2+;GPL-3+;LGPL;LGPL-2+;LGPL-2.1+;LGPL-3+;MIT;public-domain |
| libbrotli1 | 1.0.9-2+deb12u1 | brotli | MIT |
| libbsd0 | 0.11.7-2 | libbsd | BSD-2-clause;BSD-2-clause-NetBSD;BSD-2-clause-author;BSD-2-clause-verbatim;BSD-3-clause;BSD-3-clause-John-Birrell;BSD-3-clause-Regents;BSD-3-clause-Regents and BSD-2-clause-NetBSD;BSD-3-clause-author;BSD-4-clause-Niels-Provos;Beerware;Expat;ISC;ISC-Original;libutil-David-Nugent;public-domain |
| libbz2-1.0 | 1.0.8-5+b1 | bzip2 | BSD-variant;GPL-2 |
| libc-ares2 | 1.18.1-3 | c-ares | (free-form copyright file) |
| libc-bin | 2.36-9+deb12u14 | glibc | (free-form copyright file) |
| libc6 | 2.36-9+deb12u14 | glibc | (free-form copyright file) |
| libcap-ng0 | 0.8.3-1+b3 | libcap-ng | GPL-2+;GPL-3;LGPL-2.1+ |
| libcap2 | 1:2.66-4+deb12u3+b1 | libcap2 | BSD-3-clause;BSD-3-clause or GPL-2;BSD-3-clause or GPL-2+;GPL-2;GPL-2+ |
| libcbor0.8 | 0.8.0-2+b1 | libcbor | Expat |
| libcom-err2 | 1.47.0-2+b2 | e2fsprogs | Apache-2;BSD-3-Clause;GPL or MIT-US-export;GPL-2;GPL-2+ with Texinfo exception;ISC;Kazlib;LGPL-2;Latex2e |
| libcrypt1 | 1:4.4.33-2 | libxcrypt | (free-form copyright file) |
| libcurl3-gnutls | 7.88.1-10+deb12u15 | curl | BSD-3-Clause;BSD-3-clause;BSD-4-Clause-UC;FSFULLR;GPL-2+ with Autoconf-data exception;GPL-2+ with Libtool exception;GPL-3+ with Autoconf-data exception;ISC;OLDAP-2.8;X11;curl |
| libdb5.3 | 5.3.28+dfsg2-1 | db5.3 | Artistic or BSD-3-clause;BSD-3-clause;BSD-3-clause-fjord;GPL;GPL or Artistic;GPL-3;MIT-old;Ms-PL;Sleepycat;Sleepycat and BSD-3-clause;TCL-like;X11;zlib |
| libdebconfclient0 | 0.270 | cdebconf | BSD-2-Clause;BSD-2-clause;GPL-2+ |
| libedit2 | 3.1-20221030-2 | libedit | BSD-3-clause |
| liberror-perl | 0.17029-2 | liberror-perl | Artistic;Artistic or GPL-1+;GPL-1+;MIT/X11 |
| libexpat1 | 2.5.0-1+deb12u5 | expat | MIT |
| libext2fs2 | 1.47.0-2+b2 | e2fsprogs | Apache-2;BSD-3-Clause;GPL or MIT-US-export;GPL-2;GPL-2+ with Texinfo exception;ISC;Kazlib;LGPL-2;Latex2e |
| libffi8 | 3.4.4-1 | libffi | Expat;GPL;GPL-2+;GPL-3+;MPL-1.1 or GPL-2+ or LGPL-2.1+;X11;public-domain |
| libfido2-1 | 1.12.0-2+b1 | libfido2 | BSD-2-clause;ISC;ISC and BSD-2-clause;public-domain |
| libgcc-s1 | 12.2.0-14+deb12u1 | gcc-12 | (free-form copyright file) |
| libgcrypt20 | 1.10.1-3+deb12u1 | libgcrypt20 | (free-form copyright file) |
| libgdbm-compat4 | 1.23-3 | gdbm | GFDL-NIV-1.3+;GPL-2+;GPL-3+ |
| libgdbm6 | 1.23-3 | gdbm | GFDL-NIV-1.3+;GPL-2+;GPL-3+ |
| libgmp10 | 2:6.2.1+dfsg1-1.1 | gmp | GPL-2+;GPL-2+ or LGPL-3+;GPL-3+;GPL-3+ with Bison exception;LGPL-3+ |
| libgnutls30 | 3.7.9-2+deb12u7 | gnutls28 | Apache-2.0;BSD-3-Clause;CC0 license;Expat;GPLv3+;LGPLv2.1+;LGPLv3+_or_GPLv2+;The main library is licensed under GNU Lesser |
| libgpg-error0 | 1.46-1 | libgpg-error | BSD-3-clause;GPL-3+;LGPL-2.1+;LGPL-2.1+ or BSD-3-clause;g10-permissive |
| libgssapi-krb5-2 | 1.20.1-2+deb12u5 | krb5 | (free-form copyright file) |
| libhogweed6 | 3.8.1-2 | nettle | Expat;GAP;GPL-2;GPL-2+;GPL-3+;GPL-3+ with Autoconf exception;LGPL-2+;LGPL-3+;LGPL-3+ or GPL-2+;public-domain |
| libicu72 | 72.1-3+deb12u1 | icu | GPL-3;MIT |
| libidn2-0 | 2.3.3-1+b1 | libidn2 | GPL-2+;GPL-3+;LGPL-3+;LGPL-3+ or GPL-2+;Unicode |
| libjs-async | 0.8.0-6 | node-async | Expat;GPL-3+ |
| libjs-events | 3.3.0+~3.0.0-3 | node-events | Expat |
| libjs-inherits | 2.0.4-6 | node-inherits | ISC |
| libjs-is-typedarray | 1.0.0-4 | node-is-typedarray | Expat |
| libjs-prettify | 2015.12.04+dfsg-1.1 | prettify.js | Apache-2.0;Expat |
| libjs-regenerate | 1.4.2-3 | node-regenerate | Expat |
| libjs-source-map | 0.7.0++dfsg2+really.0.6.1-15 | node-source-map | BSD-3-clause |
| libjs-sprintf-js | 1.1.2+ds1+~1.1.2-1 | node-sprintf-js | BSD-3-Clause;Expat |
| libjs-typedarray-to-buffer | 4.0.0-2 | node-typedarray-to-buffer | Expat |
| libjs-util | 0.12.5+~1.0.10-1 | node-util | Expat |
| libk5crypto3 | 1.20.1-2+deb12u5 | krb5 | (free-form copyright file) |
| libkeyutils1 | 1.6.3-2 | keyutils | GPL-2+;LGPL-2+ |
| libkrb5-3 | 1.20.1-2+deb12u5 | krb5 | (free-form copyright file) |
| libkrb5support0 | 1.20.1-2+deb12u5 | krb5 | (free-form copyright file) |
| libldap-2.5-0 | 2.5.13+dfsg-5 | openldap | BSD-3-clause;BSD-3-clause-California;BSD-3-clause-variant;BSD-4-clause-California;Beerware;Expat;Expat-ISC;Expat-UNM;F5;FSF-unlimited;FSF-unlimited and GPL-2+ with Autoconf exception;FSF-unlimited and GPL-2+ with Libtool exception;FSF-unlimited and OpenLDAP-2.8;GPL-2+;GPL-2+ with Autoconf exception;GPL-2+ with Libtool exception;GPL-2+ with Libtool exception and GPL-3+ with Libtool exception and GPL-3+;GPL-3+;GPL-3+ with Autoconf exception;GPL-3+ with Libtool exception;JCG;MIT-XC;NeoSoft-permissive;OpenLDAP-2.8;OpenLDAP-2.8 and BSD-3-clause;OpenLDAP-2.8 and BSD-3-clause-variant;OpenLDAP-2.8 and BSD-4-clause-California;OpenLDAP-2.8 and Beerware;OpenLDAP-2.8 and Expat;OpenLDAP-2.8 and Expat-ISC;OpenLDAP-2.8 and Expat-UNM;OpenLDAP-2.8 and FSF-unlimited and GPL-2+ with Libtool exception;OpenLDAP-2.8 and JCG and UMich;OpenLDAP-2.8 and UMich;OpenLDAP-2.8 and UMich and F5;UMich;public-domain |
| liblz4-1 | 1.9.4-1 | lz4 | BSD-2-clause;GPL-2+;GPL-2+ or BSD-2-clause |
| liblzma5 | 5.4.1-1+deb12u2 | xz-utils | Autoconf;GPL-2;GPL-2+;LGPL-2.1+;PD;PD-debian;config-h;noderivs;none;permissive-fsf;permissive-nowarranty;probably-PD |
| libmd0 | 1.0.4-2 | libmd | BSD-2-clause;BSD-2-clause-NetBSD;BSD-3-clause;BSD-3-clause-Aaron-D-Gifford;Beerware;ISC;public-domain-md4;public-domain-md5;public-domain-sha1 |
| libmount1 | 2.38.1-5+deb12u3 | util-linux | BSD-3-clause;BSD-4-clause;BSLA;GPL-2;GPL-2+;GPL-3+;LGPL;LGPL-2+;LGPL-2.1+;LGPL-3+;MIT;public-domain |
| libncursesw6 | 6.4-4 | ncurses | BSD-3-clause;MIT/X11;X11 |
| libnettle8 | 3.8.1-2 | nettle | Expat;GAP;GPL-2;GPL-2+;GPL-3+;GPL-3+ with Autoconf exception;LGPL-2+;LGPL-3+;LGPL-3+ or GPL-2+;public-domain |
| libnghttp2-14 | 1.52.0-1+deb12u3 | nghttp2 | BSD-2-clause;Expat;GPL-3+ with autoconf exception;MIT;all-permissive |
| libnode-dev | 18.20.4+dfsg-1~deb12u3 | nodejs | Apache-2.0;BSD-2-clause and Expat;BSD-2-clause~author;BSD-2-clause~holders;BSD-3-clause;BSD-3-clause~Google;BSD-3-clause~Intel;BSD-3-clause~Intel or GPL-2;BSD-3-clause~Sun;BSD-3-clause~author;BSD-3-clause~contributors;BSD-3-clause~contributors or W3C-Test-Suite;BSD-3-clause~holders;BSD-3-clause~holders or GPL-2;BSD-3-clause~intel or GPL-2;BSD-4-clause~author;CC0;CC0 or BSD-2-clause~holders;Expat;Expat or Apache-2.0;GPL-2;GPL-3;ISC;ISC-no-disclaim;MIT/X11;MPL-2.0;OpenSSL;OpenSSL and SSLeay;Python;SSLeay;W3C-Test-Suite;curl;zlib |
| libnode108 | 18.20.4+dfsg-1~deb12u3 | nodejs | Apache-2.0;BSD-2-clause and Expat;BSD-2-clause~author;BSD-2-clause~holders;BSD-3-clause;BSD-3-clause~Google;BSD-3-clause~Intel;BSD-3-clause~Intel or GPL-2;BSD-3-clause~Sun;BSD-3-clause~author;BSD-3-clause~contributors;BSD-3-clause~contributors or W3C-Test-Suite;BSD-3-clause~holders;BSD-3-clause~holders or GPL-2;BSD-3-clause~intel or GPL-2;BSD-4-clause~author;CC0;CC0 or BSD-2-clause~holders;Expat;Expat or Apache-2.0;GPL-2;GPL-3;ISC;ISC-no-disclaim;MIT/X11;MPL-2.0;OpenSSL;OpenSSL and SSLeay;Python;SSLeay;W3C-Test-Suite;curl;zlib |
| libnsl2 | 1.3.0-2 | libnsl | BSD-3-clause;GPL-2+-autoconf-exception;GPL-2+-libtool-exception;GPL-3+-autoconf-exception;LGPL-2.1;LGPL-2.1+;MIT;permissive-autoconf-m4;permissive-autoconf-m4-no-warranty;permissive-configure;permissive-fsf;permissive-makefile-in |
| libp11-kit0 | 0.24.1-2 | p11-kit | Apache-2.0;BSD-3-Clause;ISC;ISC+IBM;LGPL-2.1+;permissive-like-automake-output;same-as-rest-of-p11kit |
| libpam-modules | 1.5.2-6+deb12u2 | pam | BSD-3-clause or GPL;BSD-tcp_wrappers;Beerware;GPL-2;GPL-2+;GPL-3;GPL-3+ with Bison exception;LGPL-2+;public-domain |
| libpam-modules-bin | 1.5.2-6+deb12u2 | pam | BSD-3-clause or GPL;BSD-tcp_wrappers;Beerware;GPL-2;GPL-2+;GPL-3;GPL-3+ with Bison exception;LGPL-2+;public-domain |
| libpam-runtime | 1.5.2-6+deb12u2 | pam | BSD-3-clause or GPL;BSD-tcp_wrappers;Beerware;GPL-2;GPL-2+;GPL-3;GPL-3+ with Bison exception;LGPL-2+;public-domain |
| libpam0g | 1.5.2-6+deb12u2 | pam | BSD-3-clause or GPL;BSD-tcp_wrappers;Beerware;GPL-2;GPL-2+;GPL-3;GPL-3+ with Bison exception;LGPL-2+;public-domain |
| libpcre2-8-0 | 10.42-1+deb12u2 | pcre2 | BSD-2-clause;BSD-3-clause;BSD-3-clause-Cambridge with BINARY LIBRARY-LIKE PACKAGES exception;X11;public-domain |
| libperl5.36 | 5.36.0-7+deb12u4 | perl | Artistic;Artistic or GPL-1+ or Artistic-dist;Artistic-2;Artistic-dist;BSD-3-clause;BSD-3-clause-GENERIC;BSD-3-clause-with-weird-numbering;BSD-4-clause-POWERDOG;BZIP;CC0-1.0;DONT-CHANGE-THE-GPL;Expat;Expat or GPL-1+ or Artistic;GPL-1+;GPL-1+ or Artistic;GPL-1+ or Artistic or Artistic-dist;GPL-1+ or Artistic, and BSD-3-clause-GENERIC;GPL-1+ or Artistic, and BSD-4-clause-POWERDOG;GPL-1+ or Artistic, and Expat;GPL-1+ or Artistic, and Unicode;GPL-2+;GPL-2+ or Artistic;GPL-3+-WITH-BISON-EXCEPTION;HSIEH-BSD;HSIEH-DERIVATIVE;LGPL-2.1;REGCOMP;REGCOMP, and GPL-1+ or Artistic;RRA-KEEP-THIS-NOTICE;SDBM-PUBLIC-DOMAIN;TEXT-TABS;Unicode;ZLIB |
| libpsl5 | 0.21.2-1 | libpsl | Chromium;MIT;gnulib |
| libpython3-stdlib | 3.11.2-1+b1 | python3-defaults | (free-form copyright file) |
| libpython3.11-minimal | 3.11.2-6+deb12u9 | python3.11 | * Permission to use this software in any way is granted without;By obtaining, using, and/or copying this software and/or its;Permission  is  hereby granted,  free  of charge,  to  any person;Permission is hereby granted, free of charge, to any person obtaining;Permission to use, copy, modify, and distribute this software and;Permission to use, copy, modify, and distribute this software and its;Permission to use, copy, modify, and distribute this software for any;Redistribution and use in source and binary forms, with or without;This software is provided 'as-is', without any express or implied;This software is provided as-is, without express or implied |
| libpython3.11-stdlib | 3.11.2-6+deb12u9 | python3.11 | * Permission to use this software in any way is granted without;By obtaining, using, and/or copying this software and/or its;Permission  is  hereby granted,  free  of charge,  to  any person;Permission is hereby granted, free of charge, to any person obtaining;Permission to use, copy, modify, and distribute this software and;Permission to use, copy, modify, and distribute this software and its;Permission to use, copy, modify, and distribute this software for any;Redistribution and use in source and binary forms, with or without;This software is provided 'as-is', without any express or implied;This software is provided as-is, without express or implied |
| libreadline8 | 8.2-1.3 | readline | GFDL-NIV-1.3+;GPL-2+;GPL-3;GPL-3+;ISC-no-attribution |
| librtmp1 | 2.4+20151223.gitfa8646d.1-2+b2 | rtmpdump | (free-form copyright file) |
| libsasl2-2 | 2.1.28+dfsg-10+deb12u1 | cyrus-sasl2 | BSD-2-clause;BSD-2-clause and MIT-CMU;BSD-2.2-clause;BSD-3-clause;BSD-3-clause-JANET;BSD-3-clause-JANET and BSD-4-clause;BSD-3-clause-PADL;BSD-3-clause-PADL and MIT-OpenVision;BSD-4-clause;BSD-4-clause and BSD-4-clause-KTH;BSD-4-clause and IBM-as-is;BSD-4-clause and MIT-Export;BSD-4-clause-KTH;BSD-4-clause-UC;FSFULLR;FSFULLR and MIT-CMU;GPL-3;GPL-3+;IBM-as-is;MIT-CMU;MIT-Export;MIT-OpenVision;OpenLDAP;OpenSSL;OpenSSL and SSLeay;RSA-MD;SSLeay |
| libsasl2-modules-db | 2.1.28+dfsg-10+deb12u1 | cyrus-sasl2 | BSD-2-clause;BSD-2-clause and MIT-CMU;BSD-2.2-clause;BSD-3-clause;BSD-3-clause-JANET;BSD-3-clause-JANET and BSD-4-clause;BSD-3-clause-PADL;BSD-3-clause-PADL and MIT-OpenVision;BSD-4-clause;BSD-4-clause and BSD-4-clause-KTH;BSD-4-clause and IBM-as-is;BSD-4-clause and MIT-Export;BSD-4-clause-KTH;BSD-4-clause-UC;FSFULLR;FSFULLR and MIT-CMU;GPL-3;GPL-3+;IBM-as-is;MIT-CMU;MIT-Export;MIT-OpenVision;OpenLDAP;OpenSSL;OpenSSL and SSLeay;RSA-MD;SSLeay |
| libseccomp2 | 2.5.4-1+deb12u1 | libseccomp | LGPL-2.1 |
| libselinux1 | 3.4-1+b6 | libselinux | (free-form copyright file) |
| libsemanage-common | 3.4-1 | libsemanage | (free-form copyright file) |
| libsemanage2 | 3.4-1+b5 | libsemanage | (free-form copyright file) |
| libsepol2 | 3.4-2.1 | libsepol | GPL-2;GPL-2+;LGPL-2.1+;Zlib |
| libsmartcols1 | 2.38.1-5+deb12u3 | util-linux | BSD-3-clause;BSD-4-clause;BSLA;GPL-2;GPL-2+;GPL-3+;LGPL;LGPL-2+;LGPL-2.1+;LGPL-3+;MIT;public-domain |
| libsqlite3-0 | 3.40.1-2+deb12u2 | sqlite3 | GPL-2+;public-domain |
| libss2 | 1.47.0-2+b2 | e2fsprogs | Apache-2;BSD-3-Clause;GPL or MIT-US-export;GPL-2;GPL-2+ with Texinfo exception;ISC;Kazlib;LGPL-2;Latex2e |
| libssh2-1 | 1.10.0-3+deb12u1 | libssh2 | BSD3 |
| libssl-dev | 3.0.22-1~deb12u2 | openssl | Apache-2.0;Artistic;Artistic or GPL-1+;GPL-1+ |
| libssl3 | 3.0.22-1~deb12u2 | openssl | Apache-2.0;Artistic;Artistic or GPL-1+;GPL-1+ |
| libstdc++6 | 12.2.0-14+deb12u1 | gcc-12 | (free-form copyright file) |
| libsystemd0 | 252.39-1~deb12u2 | systemd | CC0-1.0;Expat;GPL-2 with Linux-syscall-note exception;GPL-2+;LGPL-2.1+;public-domain |
| libtasn1-6 | 4.19.0-2+deb12u1 | libtasn1-6 | (free-form copyright file) |
| libtinfo6 | 6.4-4 | ncurses | BSD-3-clause;MIT/X11;X11 |
| libtirpc-common | 1.3.3+ds-1 | libtirpc | BSD-2-Clause;BSD-3-Clause;BSD-4-Clause;GPL-2;LGPL-2.1+;PERMISSIVE;__AUTO_PERMISSIVE__ |
| libtirpc3 | 1.3.3+ds-1 | libtirpc | BSD-2-Clause;BSD-3-Clause;BSD-4-Clause;GPL-2;LGPL-2.1+;PERMISSIVE;__AUTO_PERMISSIVE__ |
| libudev1 | 252.39-1~deb12u2 | systemd | CC0-1.0;Expat;GPL-2 with Linux-syscall-note exception;GPL-2+;LGPL-2.1+;public-domain |
| libunistring2 | 1.0-2 | libunistring | FreeSoftware;GFDL-1.2+;GPL-2+;GPL-2+ with distribution exception;GPL-3+;GPL-3+ or GFDL-1.2+;LGPL-3+;LGPL-3+ or GPL-2+;MIT |
| libuuid1 | 2.38.1-5+deb12u3 | util-linux | BSD-3-clause;BSD-4-clause;BSLA;GPL-2;GPL-2+;GPL-3+;LGPL;LGPL-2+;LGPL-2.1+;LGPL-3+;MIT;public-domain |
| libuv1 | 1.44.2-1+deb12u1 | libuv1 | Apache-2.0;BSD-2-clause;BSD-3-clause;CC-BY-4.0;Expat;GPL3+ with autoconf exception;ISC |
| libuv1-dev | 1.44.2-1+deb12u1 | libuv1 | Apache-2.0;BSD-2-clause;BSD-3-clause;CC-BY-4.0;Expat;GPL3+ with autoconf exception;ISC |
| libxxhash0 | 0.8.1-1 | xxhash | BSD-2-clause;GPL-2 |
| libzstd1 | 1.5.4+dfsg2-5 | libzstd | BSD-3-clause;BSD-3-clause or GPL-2;Expat;GPL-2;zlib |
| login | 1:4.13+dfsg1-1+deb12u2 | shadow | BSD-3-clause;GPL-1;GPL-2+;public-domain |
| logsave | 1.47.0-2+b2 | e2fsprogs | Apache-2;BSD-3-Clause;GPL or MIT-US-export;GPL-2;GPL-2+ with Texinfo exception;ISC;Kazlib;LGPL-2;Latex2e |
| mawk | 1.3.4.20200120-3.1 | mawk | CC-BY-3.0;GPL-2;X11 |
| media-types | 10.0.0 | media-types | ad-hoc |
| mount | 2.38.1-5+deb12u3 | util-linux | BSD-3-clause;BSD-4-clause;BSLA;GPL-2;GPL-2+;GPL-3+;LGPL;LGPL-2+;LGPL-2.1+;LGPL-3+;MIT;public-domain |
| ncurses-base | 6.4-4 | ncurses | BSD-3-clause;MIT/X11;X11 |
| ncurses-bin | 6.4-4 | ncurses | BSD-3-clause;MIT/X11;X11 |
| node-abbrev | 1.1.1+~1.1.2-1 | node-abbrev | Expat;Expat or ISC;ISC |
| node-acorn | 8.8.1+ds+~cs25.17.7-2 | acorn | Apache-2.0;BSD-2-clause;Expat |
| node-agent-base | 6.0.2+~cs5.4.2-2 | node-agent-base | Expat |
| node-ajv | 6.12.6-3 | node-ajv | Expat |
| node-ajv-keywords | 3.5.2-2 | node-ajv-keywords | Expat |
| node-ampproject-remapping | 2.2.0+~cs5.15.37-1 | node-ampproject-remapping | Apache-2.0;Expat |
| node-ansi-escapes | 5.0.0+really.4.3.1-1 | node-ansi-escapes | Expat;GPL-3.0+ |
| node-ansi-regex | 5.0.1-1 | node-ansi-regex | Expat |
| node-ansi-styles | 6.2.1-2 | node-ansi-styles | Expat |
| node-anymatch | 3.1.3+~cs4.6.1-2 | node-anymatch | Expat;ISC |
| node-aproba | 2.0.0-3 | node-aproba | ISC |
| node-archy | 1.0.0-6 | node-archy | Expat |
| node-are-we-there-yet | 3.0.1+~1.1.0-1 | node-are-we-there-yet | Expat;ISC |
| node-argparse | 2.0.1-2 | node-argparse | Expat |
| node-arrify | 2.0.1-3 | node-arrify | Expat |
| node-assert | 2.0.0+~cs3.9.8-2 | node-assert | Expat |
| node-async | 0.8.0-6 | node-async | Expat;GPL-3+ |
| node-async-each | 1.0.3-3 | node-async-each | Expat |
| node-babel-helper-define-polyfill-provider | 0.3.3~0~20220913+ds1-1 | node-babel-polyfills | Expat |
| node-babel-plugin-add-module-exports | 1.0.4+dfsg1~cs5.8.0-4 | node-babel-plugin-add-module-exports | Expat |
| node-babel-plugin-lodash | 3.3.4+~cs2.0.1-6 | node-babel-plugin-lodash | CC0-1.0;Expat;Expat or CC0-1.0 |
| node-babel-plugin-polyfill-corejs2 | 0.3.3~0~20220913+ds1-1 | node-babel-polyfills | Expat |
| node-babel-plugin-polyfill-corejs3 | 0.6.0~0~20220913+ds1-1 | node-babel-polyfills | Expat |
| node-babel-plugin-polyfill-regenerator | 0.4.1~0~20220913+ds1-1 | node-babel-polyfills | Expat |
| node-babel7 | 7.20.15+ds1+~cs214.269.168-3+deb12u2 | node-babel7 | Apache-2.0;BSD-2-Clause;BSD-3-Clause;Expat |
| node-babel7-runtime | 7.20.15+ds1+~cs214.269.168-3+deb12u2 | node-babel7 | Apache-2.0;BSD-2-Clause;BSD-3-Clause;Expat |
| node-balanced-match | 2.0.0-1 | node-balanced-match | Expat |
| node-base64-js | 1.5.1+dfsg+~1.3.0-2 | node-base64-js | Expat |
| node-binary-extensions | 2.2.0-2 | node-binary-extensions | Expat |
| node-brace-expansion | 2.0.1-2 | node-brace-expansion | Expat |
| node-braces | 3.0.2+~3.0.1-1 | node-braces | Expat |
| node-browserslist | 4.21.4+~cs6.1.17-2 | node-browserslist | Expat |
| node-builtins | 5.0.1-1 | node-builtins | Expat |
| node-busboy | 1.6.0+~cs2.6.0-2 | node-busboy | Expat |
| node-cacache | 17.0.3+~cs10.3.7-1 | node-cacache | Expat;ISC |
| node-camelcase | 7.0.0-3 | node-camelcase | Expat |
| node-caniuse-lite | 1.0.30001436+dfsg+~1.0.1-1 | node-caniuse-lite | CC-BY-4.0;Expat |
| node-chalk | 5.2.0-1 | node-chalk | Expat |
| node-chokidar | 3.5.3-2 | node-chokidar | Expat |
| node-chownr | 2.0.0-2 | node-chownr | ISC |
| node-chrome-trace-event | 1.0.3-2 | node-chrome-trace-event | Expat |
| node-ci-info | 3.6.1+~cs1.1.0-1 | node-ci-info | Expat |
| node-cjs-module-lexer | 1.2.2+dfsg-5 | node-cjs-module-lexer | Expat |
| node-cli-table | 0.3.11+~cs0.13.4-3 | node-cli-table | Expat |
| node-cliui | 7.0.4+repack+~cs3.1.0-4 | node-cliui | Apache-2.0;GPL-3.0+;ISC |
| node-clone | 2.1.2-4 | node-clone | Expat |
| node-clone-deep | 4.0.1+~cs7.0.2-1 | node-clone-deep | Expat |
| node-color-convert | 2.0.1+~cs2.0.0-2 | node-color-convert | Expat |
| node-color-name | 1.1.4+~1.1.1-2 | node-color-name | Expat |
| node-colors | 1.4.0-4 | colors.js | Expat |
| node-columnify | 1.6.0+~1.5.1-1 | node-columnify | Expat |
| node-commander | 9.4.1-1 | node-commander | Expat |
| node-commondir | 1.0.1+~1.0.0-1 | node-commondir | Expat |
| node-concat-stream | 2.0.0+~2.0.0-2 | node-concat-stream | Expat |
| node-console-control-strings | 1.1.0-3 | node-console-control-strings | ISC |
| node-convert-source-map | 1.9.0+~1.5.2-1 | node-convert-source-map | Expat |
| node-copy-concurrently | 1.0.5-9 | node-copy-concurrently | ISC |
| node-core-js | 3.26.1-3 | node-core-js | BSD-3-Clause;Expat |
| node-core-js-compat | 3.26.1-3 | node-core-js | BSD-3-Clause;Expat |
| node-core-js-pure | 3.26.1-3 | node-core-js | BSD-3-Clause;Expat |
| node-core-util-is | 1.0.3-1 | node-core-util-is | Expat |
| node-css-loader | 6.7.2+~cs14.0.11-1 | node-css-loader | Expat;ISC |
| node-css-selector-tokenizer | 0.8.0+~cs4.8.3-1 | node-css-selector-tokenizer | Expat |
| node-data-uri-to-buffer | 4.0.0-3 | node-data-uri-to-buffer | Expat |
| node-debbundle-es-to-primitive | 1.2.1+~cs9.7.25-2 | node-debbundle-es-to-primitive | Expat |
| node-debug | 4.3.4+~cs4.1.7-1 | node-debug | Expat |
| node-decamelize | 4.0.0-2 | node-decamelize | Expat |
| node-decompress-response | 6.0.0-2 | node-decompress-response | Expat |
| node-deep-equal | 2.1.0+~cs31.12.80-1 | node-deep-equal | Expat |
| node-deep-is | 0.1.4-1 | node-deep-is | Expat |
| node-defaults | 1.0.4+~1.0.3-1 | node-defaults | Expat |
| node-define-properties | 1.1.4+~1.1.3-1 | node-define-properties | Expat |
| node-defined | 1.0.1+~1.0.0-1 | node-defined | Expat |
| node-del | 7.0.0-5 | node-del | Expat |
| node-delegates | 1.0.0-3 | node-delegates | Expat |
| node-depd | 2.0.0-2 | node-depd | Expat |
| node-diff | 5.0.0~dfsg+~5.0.1-4 | node-diff | BSD-3-clause;Expat;GPL-3+ |
| node-doctrine | 3.0.0-4 | node-doctrine | Apache-2.0 |
| node-electron-to-chromium | 1.4.284-1 | node-electron-to-chromium | ISC |
| node-encoding | 0.1.13-2 | node-encoding | Expat |
| node-end-of-stream | 1.4.4+~1.4.1-1 | node-end-of-stream | Expat |
| node-enhanced-resolve | 5.10.0-1 | node-enhanced-resolve | Expat |
| node-err-code | 2.0.3+dfsg-3 | node-err-code | Expat |
| node-errno | 1.0.0-3 | node-errno | Expat |
| node-error-ex | 1.3.2-4 | node-error-ex | Expat |
| node-es-abstract | 1.20.4+~cs26.27.47-1 | node-es-abstract | Expat |
| node-es-module-lexer | 1.1.0+dfsg-2+deb12u1 | node-es-module-lexer | Expat |
| node-es6-error | 4.1.1-5 | node-es6-error | Expat |
| node-escape-string-regexp | 4.0.0-2 | node-escape-string-regexp | Expat |
| node-escodegen | 2.0.0+dfsg+~0.0.7-2 | node-escodegen | BSD-2-Clause;Expat |
| node-eslint-scope | 7.1.1+~3.7.4-1 | node-eslint-scope | BSD-2-clause~HOLDER;Expat;GPL-3+ |
| node-eslint-utils | 3.0.0-3 | node-eslint-utils | Expat;GPL-3+ |
| node-eslint-visitor-keys | 3.3.0+~1.0.0-1 | node-eslint-visitor-keys | Apache-2.0;Expat;GPL-3+ |
| node-espree | 9.4.1~dfsg-1 | node-espree | BSD-2-Clause;BSD-2-Clause and Expat;BSD-3-Clause~Ecma;Expat;GPL-3+ |
| node-esprima | 4.0.1+ds+~4.0.3-2 | node-esprima | BSD-2-Clause;BSD-2-clause;Expat |
| node-esquery | 1.4.2~ds-1 | node-esquery | BSD-3-clause;GPL-3+ |
| node-esrecurse | 4.3.0-2 | node-esrecurse | BSD-2-Clause |
| node-estraverse | 5.3.0+ds+~5.1.1-1 | node-estraverse | BSD-2-Clause;BSD-2-clause;Expat |
| node-esutils | 2.0.3+~2.0.0-1 | node-esutils | BSD-2-clause;Expat |
| node-events | 3.3.0+~3.0.0-3 | node-events | Expat |
| node-fancy-log | 1.3.3+~cs1.3.1-2 | node-fancy-log | Expat;ISC |
| node-fast-deep-equal | 3.1.3-3 | node-fast-deep-equal | Expat |
| node-fast-levenshtein | 2.0.6+ds-3 | node-fast-levenshtein | Expat |
| node-fetch | 3.3.0+~cs11.4.11-2 | node-fetch | Expat |
| node-file-entry-cache | 6.0.0+~3.0.4+~2.0.0+~1.0.0+~2.0.1-5 | node-file-entry-cache | Expat;GPL-3+ |
| node-fill-range | 7.0.1+~7.0.0-1 | node-fill-range | Expat |
| node-find-cache-dir | 3.3.2+~3.2.1-1 | node-find-cache-dir | Expat |
| node-find-up | 6.3.0-7 | node-find-up | Expat |
| node-flat-cache | 3.0.4~6.0.0+~3.0.4+~2.0.0+~1.0.0+~2.0.1-5 | node-file-entry-cache | Expat;GPL-3+ |
| node-flatted | 3.2.7~ds-1+deb12u1 | node-flatted | GPL-3+;ISC;public-domain |
| node-for-in | 1.0.2-4 | node-for-in | Expat |
| node-for-own | 1.0.0-3 | node-for-own | Expat |
| node-foreground-child | 2.0.0-5 | node-foreground-child | ISC |
| node-fs-readdir-recursive | 1.1.0+~1.1.0-1 | node-fs-readdir-recursive | Expat |
| node-fs-write-stream-atomic | 1.0.10-6 | node-fs-write-stream-atomic | ISC |
| node-fs.realpath | 1.0.0-3 | node-fs.realpath | Expat;ISC |
| node-function-bind | 1.1.1+repacked+~1.0.3-2 | node-function-bind | Expat |
| node-functional-red-black-tree | 1.0.1+20181105-7 | node-functional-red-black-tree | Expat;GPL-3+ |
| node-gauge | 4.0.4-1 | node-gauge | ISC |
| node-get-caller-file | 2.0.5+~cs1.1.1-4 | node-get-caller-file | ISC |
| node-get-stream | 6.0.1-1 | node-get-stream | Expat |
| node-glob | 8.0.3+~cs8.4.15-1 | node-glob | CC-BY-SA-4.0;Expat;GPL-2+;ISC |
| node-glob-parent | 6.0.2+~5.1.1-2 | node-glob-parent | Expat;ISC |
| node-globals | 13.18.0-1 | node-globals | Expat |
| node-globby | 13.1.3+~cs16.25.40-1+deb12u1 | node-globby | Expat;ISC |
| node-got | 11.8.5+~cs58.13.36-3 | node-got | Apache-2.0;BSD-2-Clause;Expat |
| node-graceful-fs | 4.2.10-1 | node-graceful-fs | BSD-2-clause;Expat;ISC |
| node-gyp | 9.3.0-2 | node-gyp | BSD-3-clause~Google;Expat |
| node-has-flag | 4.0.0-3 | node-has-flag | Expat |
| node-has-unicode | 2.0.1-5 | node-has-unicode | ISC |
| node-hosted-git-info | 6.1.1-2 | node-hosted-git-info | ISC |
| node-https-proxy-agent | 5.0.1+~cs8.0.0-3 | node-https-proxy-agent | Expat |
| node-iconv-lite | 0.6.3-3 | node-iconv-lite | Expat |
| node-icss-utils | 5.1.0+~5.1.0-1 | node-icss-utils | Expat;ISC |
| node-ieee754 | 1.2.1-3 | node-ieee754 | BSD-3-Clause |
| node-iferr | 1.0.2+~1.0.2-1 | node-iferr | Expat |
| node-ignore | 5.2.1-1 | node-ignore | Expat;GPL-3+ |
| node-imurmurhash | 0.1.4+dfsg+~0.1.1-2 | node-imurmurhash | Expat |
| node-indent-string | 4.0.0-2 | node-indent-string | Expat |
| node-inflight | 1.0.6-2 | node-inflight | ISC |
| node-inherits | 2.0.4-6 | node-inherits | ISC |
| node-ini | 3.0.1-2 | node-ini | Expat |
| node-interpret | 2.2.0-3 | node-interpret | Expat |
| node-ip | 2.0.0+~1.1.0-1 | node-ip | Expat |
| node-ip-regex | 4.3.0+~4.1.1-1 | node-ip-regex | Expat |
| node-is-arrayish | 0.3.2-3 | node-is-arrayish | Expat |
| node-is-binary-path | 2.1.0-5 | node-is-binary-path | Expat |
| node-is-buffer | 2.0.5-2 | node-is-buffer | Expat |
| node-is-extendable | 1.0.1-3 | node-is-extendable | Expat |
| node-is-extglob | 2.1.1-5 | node-is-extglob | Expat |
| node-is-glob | 4.0.3-2 | node-is-glob | Expat |
| node-is-number | 7.0.0-4 | node-is-number | Expat |
| node-is-path-cwd | 2.2.0-2 | node-is-path-cwd | Expat |
| node-is-path-inside | 3.0.3-2 | node-is-path-inside | Expat |
| node-is-plain-obj | 3.0.0-2 | node-is-plain-obj | Expat |
| node-is-plain-object | 5.0.0-7 | node-is-plain-object | Expat |
| node-is-stream | 3.0.0-4 | node-is-stream | Expat |
| node-is-typedarray | 1.0.0-4 | node-is-typedarray | Expat |
| node-is-windows | 1.0.2+~cs1.0.0-1 | node-is-windows | Expat |
| node-isarray | 2.0.5-4 | node-isarray | Expat |
| node-isexe | 2.0.0+~2.0.1-5 | node-isexe | Expat;ISC |
| node-isobject | 4.0.0-2 | node-isobject | Expat |
| node-istanbul | 0.4.5+repack10+~cs97.25.57-3 | node-istanbul | BSD-3-Clause;BSD-3-Clause-babel-plugin-istanbul;Expat;ISC |
| node-jest-debbundle | 29.3.1~ds1+~cs70.48.25-2 | node-jest | Apache-2.0;Expat;ISC |
| node-jest-worker | 29.3.1~ds1+~cs70.48.25-2 | node-jest | Apache-2.0;Expat;ISC |
| node-js-tokens | 8.0.0-2 | node-js-tokens | Apache-2.0;Expat;Expat or Apache-2.0 |
| node-js-yaml | 4.1.0+dfsg+~4.0.5-7 | node-js-yaml | Expat |
| node-jsesc | 3.0.2+~3.0.1-1 | node-jsesc | Expat |
| node-json-buffer | 3.0.1+~3.0.0-1 | node-json-buffer | Expat |
| node-json-parse-better-errors | 1.0.2+~cs3.3.1-2 | node-json-parse-better-errors | Expat |
| node-json-schema | 0.4.0+~7.0.11-1 | node-json-schema | BSD-3-clause~Dojo;Expat;GPL-3+ |
| node-json-schema-traverse | 1.0.0-3 | node-json-schema-traverse | Expat |
| node-json-stable-stringify | 1.0.2+~cs5.2.34-1 | node-json-stable-stringify | Expat |
| node-json5 | 2.2.3+dfsg-1 | node-json5 | Expat |
| node-jsonify | 0.0.1-1 | node-jsonify | public-domain |
| node-jsonparse | 1.3.1-10 | node-jsonparse | Expat |
| node-kind-of | 6.0.3+dfsg-2 | node-kind-of | Expat |
| node-levn | 0.4.1+dfsg-2 | node-levn | Expat |
| node-loader-runner | 4.3.0-1 | node-loader-runner | Expat |
| node-locate-path | 7.1.1-2 | node-locate-path | Expat;GPL-3.0+ |
| node-lodash | 4.17.21+dfsg+~cs8.31.198.20210220-9+deb12u1 | node-lodash | Expat;GPL-2+ |
| node-lodash-packages | 4.17.21+dfsg+~cs8.31.198.20210220-9+deb12u1 | node-lodash | Expat;GPL-2+ |
| node-lowercase-keys | 2.0.0-2 | node-lowercase-keys | Expat |
| node-lru-cache | 7.14.1-1 | node-lru-cache | Expat;ISC |
| node-make-dir | 3.1.0-3 | node-make-dir | Expat |
| node-memfs | 3.4.12+~cs1.0.3-1 | node-memfs | Expat;ISC;public-domain |
| node-memory-fs | 0.5.0+~0.3.3-2 | node-memory-fs | Expat |
| node-merge-stream | 2.0.0+~1.1.2-2 | node-merge-stream | Expat |
| node-micromatch | 4.0.5+~4.0.2-1 | node-micromatch | Expat;ISC |
| node-mime | 3.0.0+dfsg+~cs3.97.1-1 | node-mime | Expat |
| node-mime-types | 2.1.35-1 | node-mime-types | Expat |
| node-mimic-response | 3.1.0-8 | node-mimic-response | Expat |
| node-minimatch | 5.1.1+~5.1.2-1 | node-minimatch | Expat;ISC |
| node-minimist | 1.2.7+~cs5.3.2-2 | node-minimist | Expat |
| node-minipass | 3.3.6+~cs9.4.19-1+deb12u1 | node-minipass | Expat;ISC |
| node-mkdirp | 1.0.4+~1.0.2-4 | node-mkdirp | Expat |
| node-move-concurrently | 1.0.1-5 | node-move-concurrently | ISC |
| node-ms | 2.1.3+~cs0.7.31-3 | node-ms | Expat |
| node-mute-stream | 0.0.8+~0.0.1-3 | node-mute-stream | Expat;GPL-3.0+;ISC |
| node-n3 | 1.16.3+~1.2.3+~1.10.4-1 | node-n3 | Expat;GPL-3+ |
| node-negotiator | 0.6.3+~0.6.1-1 | node-negotiator | Expat |
| node-neo-async | 2.6.2+~cs3.0.0-2 | node-neo-async | Expat |
| node-nopt | 5.0.0-4 | node-nopt | ISC |
| node-normalize-package-data | 4.0.1+~2.4.1-1 | node-normalize-package-data | BSD-2-clause;Expat |
| node-normalize-path | 3.0.0+~3.0.0-1 | node-normalize-path | Expat |
| node-npm-bundled | 2.0.1-2 | node-npm-bundled | ISC |
| node-npm-package-arg | 10.0.0+~3.0.0-2 | node-npm-package-arg | ISC |
| node-npm-run-path | 5.1.0+~4.0.0-8 | node-npm-run-path | Expat |
| node-npmlog | 7.0.1+~4.1.4-1 | node-npmlog | Expat;ISC |
| node-object-assign | 4.1.1-7 | node-object-assign | Expat |
| node-object-inspect | 1.12.2+~cs1.8.1-1 | node-object-inspect | Expat |
| node-once | 1.4.0-7 | node-once | BSD-2-clause;ISC |
| node-optimist | 0.6.1+~0.0.30-2 | node-optimist | Expat |
| node-optionator | 0.9.1+dfsg+~cs1.2.3-1 | node-optionator | Expat |
| node-osenv | 0.1.5+~0.1.1-1 | node-osenv | BSD-2-clause;Expat |
| node-p-cancelable | 2.1.1-1 | node-p-cancelable | Expat |
| node-p-limit | 4.0.0+~cs4.0.0-5 | node-p-limit | Expat |
| node-p-locate | 6.0.0-12 | node-p-locate | Expat |
| node-p-map | 4.0.0+~3.1.0+~3.0.1-1 | node-p-map | Expat |
| node-parse-json | 5.2.0+~cs5.1.7-1 | node-parse-json | Expat |
| node-path-dirname | 1.0.2-2 | node-path-dirname | Expat |
| node-path-exists | 5.0.0-8 | node-path-exists | Expat |
| node-path-is-absolute | 2.0.0-2 | node-path-is-absolute | Expat |
| node-path-is-inside | 1.0.2+~1.0.0-1 | node-path-is-inside | Expat;WTFPL;WTFPL OR Expat |
| node-path-type | 4.0.0-3 | node-path-type | Expat |
| node-picocolors | 1.0.0-4 | node-picocolors | ISC |
| node-pify | 5.0.0+~cs5.0.1-1 | node-pify | Expat |
| node-pkg-dir | 5.0.0-2 | node-pkg-dir | Expat |
| node-postcss | 8.4.20+~cs8.0.23-1+deb12u1 | node-postcss | BSD-3-Clause;Expat |
| node-postcss-modules-extract-imports | 3.0.0-2 | node-postcss-modules-extract-imports | ISC |
| node-postcss-modules-values | 4.0.0+~4.0.0-1 | node-postcss-modules-values | Expat;ISC |
| node-postcss-value-parser | 4.2.0-1 | node-postcss-value-parser | Expat |
| node-prelude-ls | 1.2.1+dfsg-3 | node-prelude-ls | Expat |
| node-process-nextick-args | 2.0.1-3 | node-process-nextick-args | Expat |
| node-progress | 2.0.3-2 | node-progress | Expat;GPL-3.0+ |
| node-promise-inflight | 1.0.1+~1.0.0-1 | node-promise-inflight | Expat;ISC |
| node-promise-retry | 2.0.1-4 | node-promise-retry | Expat |
| node-promzard | 0.3.0-2 | node-promzard | ISC |
| node-prr | 1.0.1-3 | node-prr | Expat |
| node-pump | 3.0.0-5 | node-pump | Expat |
| node-punycode | 2.1.1-6 | node-punycode | Expat |
| node-quick-lru | 6.1.1-4 | node-quick-lru | Expat |
| node-randombytes | 2.1.0+~2.0.0-2 | node-randombytes | Expat |
| node-read | 1.0.7-5 | node-read | GPL-3.0+;ISC |
| node-read-package-json | 5.0.2+~2.0.0-1 | node-read-package-json | ISC |
| node-read-pkg | 5.2.0-2 | node-read-pkg | Expat |
| node-readable-stream | 3.6.0+~cs3.0.0-4 | node-readable-stream | Expat |
| node-readdirp | 3.6.0-1 | node-readdirp | Expat |
| node-rechoir | 0.8.0+~0.6.1-2 | node-rechoir | Expat |
| node-regenerate | 1.4.2-3 | node-regenerate | Expat |
| node-regenerate-unicode-properties | 10.1.0+ds-1 | node-regenerate-unicode-properties | Expat |
| node-regenerator-runtime | 0.15.1+~0.10.8-1 | node-regenerator | Expat |
| node-regenerator-transform | 0.15.1+~0.10.8-1 | node-regenerator | Expat |
| node-regexpp | 3.2.0-4+deb12u1 | node-regexpp | Expat;GPL-3+ |
| node-regexpu-core | 5.2.2-3 | node-regexpu-core | Expat |
| node-regjsgen | 0.7.1+ds-1 | node-regjsgen | Expat |
| node-regjsparser | 0.9.1+ds-2 | node-regjsparser | BSD-2-Clause |
| node-repeat-string | 1.6.1+repack-1 | node-repeat-string | Expat |
| node-require-directory | 2.1.1+~2.1.2-1 | node-require-directory | Expat |
| node-resolve | 1.22.1+~cs5.31.10-1 | node-resolve | Expat |
| node-resolve-cwd | 3.0.0-2 | node-resolve-cwd | Expat |
| node-resolve-from | 5.0.0+~3.1.0+~3.3.0+~2.0.0-1 | node-resolve-from | Expat |
| node-resumer | 0.0.0-6 | node-resumer | Expat |
| node-retry | 0.13.1+~0.12.1-1 | node-retry | Expat;GPL-3.0+ |
| node-rimraf | 3.0.2-2 | node-rimraf | Expat;ISC |
| node-run-queue | 2.0.0-3 | node-run-queue | ISC |
| node-safe-buffer | 5.2.1+~cs2.1.2-3 | node-safe-buffer | Expat |
| node-schema-utils | 3.1.1~ds-2 | node-schema-utils | Expat |
| node-semver | 7.3.5+~7.3.9-2 | node-semver | Expat;ISC |
| node-serialize-javascript | 6.0.0-2+deb12u1 | node-serialize-javascript | BSD-3-clause~Yahoo;GPL-3+ |
| node-set-blocking | 2.0.0-2 | node-set-blocking | ISC |
| node-set-immediate-shim | 2.0.0-2 | node-set-immediate-shim | Expat |
| node-shebang-command | 2.0.0-1 | node-shebang-command | Expat |
| node-shebang-regex | 3.0.0-2 | node-shebang-regex | Expat |
| node-signal-exit | 3.0.7+~3.0.1-1 | node-signal-exit | Expat;ISC |
| node-slash | 4.0.0-3 | node-slash | Expat |
| node-slice-ansi | 5.0.0+~cs9.0.0-5 | node-slice-ansi | Expat |
| node-source-list-map | 2.0.1+dfsg-2 | node-source-list-map | BSD-3-clause;Expat |
| node-source-map | 0.7.0++dfsg2+really.0.6.1-15 | node-source-map | BSD-3-clause |
| node-source-map-support | 0.5.21+ds+~0.5.4-1 | node-source-map-support | Expat |
| node-spdx-correct | 3.1.1-2 | node-spdx-correct | Apache-2.0 |
| node-spdx-exceptions | 2.3.0-2 | node-spdx-exceptions | CC-BY-3.0 |
| node-spdx-expression-parse | 3.0.1+~3.0.1-1 | node-spdx-expression-parse | CC-BY-3.0;Expat;Expat and CC-BY-3.0 |
| node-spdx-license-ids | 3.0.12-1 | node-spdx-license-ids | public-domain |
| node-sprintf-js | 1.1.2+ds1+~1.1.2-1 | node-sprintf-js | BSD-3-Clause;Expat |
| node-ssri | 9.0.1-2 | node-ssri | CC0-1.0;ISC |
| node-string-decoder | 1.3.0-6 | node-string-decoder | Expat |
| node-string-width | 4.2.3+~cs13.2.3-1 | node-string-width | Expat;GPL-3.0+ |
| node-strip-ansi | 6.0.1-2 | node-strip-ansi | Expat |
| node-strip-bom | 4.0.0-2 | node-strip-bom | Expat |
| node-strip-json-comments | 4.0.0-4 | node-strip-json-comments | Expat |
| node-supports-color | 8.1.1+~8.1.1-1 | node-supports-color | Expat |
| node-tapable | 2.2.1-2 | node-tapable | Expat |
| node-tape | 5.6.1+~cs8.20.19-1 | node-tape | Expat |
| node-tar | 6.1.13+~cs7.0.5-1+deb12u1 | node-tar | Expat;ISC |
| node-terser | 5.16.5-2 | node-terser | BSD-2-clause;GPL-3+ |
| node-text-table | 0.2.0-4 | node-text-table | Expat |
| node-through | 2.3.8+~cs0.0.30-1 | node-through | Expat |
| node-time-stamp | 2.2.0-2 | node-time-stamp | Expat |
| node-to-fast-properties | 3.0.1-3 | node-to-fast-properties | Expat |
| node-to-regex-range | 5.0.1-4 | node-to-regex-range | Expat |
| node-tslib | 2.4.1-1 | node-tslib | Apache-2.0;ISC |
| node-type-check | 0.4.0+dfsg-3 | node-type-check | Expat |
| node-typedarray | 0.0.7-1 | node-typedarray | Expat |
| node-typedarray-to-buffer | 4.0.0-2 | node-typedarray-to-buffer | Expat |
| node-undici | 5.15.0+dfsg1+~cs20.10.9.3-1+deb12u4 | node-undici | CC0-1.0;Expat |
| node-unicode-canonical-property-names-ecmascript | 2.0.0-2 | node-unicode-canonical-property-names-ecmascript | Expat |
| node-unicode-match-property-ecmascript | 2.0.0-2 | node-unicode-match-property-ecmascript | Expat |
| node-unicode-match-property-value-ecmascript | 2.1.0+ds-1 | node-unicode-match-property-value-ecmascript | Expat |
| node-unicode-property-aliases-ecmascript | 2.1.0+ds-1 | node-unicode-property-aliases-ecmascript | Expat |
| node-unique-filename | 1.1.1+ds-2 | node-unique-filename | ISC |
| node-uri-js | 4.4.0+dfsg-8 | node-uri-js | BSD-2-clause;Expat;Expat or GPL and BSD-2-clause;GPL |
| node-util | 0.12.5+~1.0.10-1 | node-util | Expat |
| node-util-deprecate | 1.0.2-3 | node-util-deprecate | Expat |
| node-uuid | 8.3.2+~8.3.3-3 | node-uuid | Expat;Expat and public-domain;GPL-3+;public-domain |
| node-v8-compile-cache | 2.3.0-3+deb12u1 | node-v8-compile-cache | Expat;GPL-3.0+ |
| node-v8flags | 3.2.0+~3.1.1-1 | node-v8flags | Expat |
| node-validate-npm-package-license | 3.0.4-2 | node-validate-npm-package-license | Apache-2.0 |
| node-validate-npm-package-name | 5.0.0+~4.0.0-1 | node-validate-npm-package-name | Expat;ISC |
| node-watchpack | 2.4.0+~cs2.8.1-1 | node-watchpack | BSD-3-Clause;Expat |
| node-wcwidth.js | 1.0.2-2 | node-wcwidth.js | Expat |
| node-webassemblyjs | 1.11.4+dfsg+~cs10.11.17-2 | node-webassemblyjs | Apache-2.0;BSD-3-clause;Expat |
| node-webpack-sources | 3.2.3+~3.2.0-2 | node-webpack-sources | Expat |
| node-which | 2.0.2+~cs1.3.2-3 | node-which | Expat;ISC |
| node-wide-align | 1.1.3-4 | node-wide-align | ISC |
| node-wordwrap | 1.0.0-4 | node-wordwrap | Expat |
| node-wrap-ansi | 8.0.1+~8.0.1-3 | node-wrap-ansi | Expat |
| node-wrappy | 1.0.2-3 | node-wrappy | ISC |
| node-write | 2.0.0~6.0.0+~3.0.4+~2.0.0+~1.0.0+~2.0.1-5 | node-file-entry-cache | Expat;GPL-3+ |
| node-write-file-atomic | 4.0.2+~4.0.0-1 | node-write-file-atomic | Expat;ISC |
| node-xtend | 4.0.2-3 | node-xtend | Expat |
| node-y18n | 5.0.8+~5.0.0-3 | node-y18n | Apache-2.0;Expat;ISC |
| node-yallist | 4.0.0+~4.0.1-1 | node-yallist | Expat;ISC |
| node-yargs | 16.2.0+~16.0.4-7 | node-yargs | Expat |
| node-yargs-parser | 21.1.1+~21.0.0-4 | node-yargs-parser | Apache-2.0;Expat;ISC |
| nodejs | 18.20.4+dfsg-1~deb12u3 | nodejs | Apache-2.0;BSD-2-clause and Expat;BSD-2-clause~author;BSD-2-clause~holders;BSD-3-clause;BSD-3-clause~Google;BSD-3-clause~Intel;BSD-3-clause~Intel or GPL-2;BSD-3-clause~Sun;BSD-3-clause~author;BSD-3-clause~contributors;BSD-3-clause~contributors or W3C-Test-Suite;BSD-3-clause~holders;BSD-3-clause~holders or GPL-2;BSD-3-clause~intel or GPL-2;BSD-4-clause~author;CC0;CC0 or BSD-2-clause~holders;Expat;Expat or Apache-2.0;GPL-2;GPL-3;ISC;ISC-no-disclaim;MIT/X11;MPL-2.0;OpenSSL;OpenSSL and SSLeay;Python;SSLeay;W3C-Test-Suite;curl;zlib |
| npm | 9.2.0~ds1-1 | npm | Apache-2.0;Artistic-2.0;BSD-2-Clause;Expat;ISC |
| openssh-client | 1:9.2p1-2+deb12u10 | openssh | BSD-2-clause;BSD-3-clause;Expat-with-advertising-restriction;Mazieres-BSD-style;OpenSSH;Powell-BSD-style;public-domain |
| openssl | 3.0.22-1~deb12u2 | openssl | Apache-2.0;Artistic;Artistic or GPL-1+;GPL-1+ |
| passwd | 1:4.13+dfsg1-1+deb12u2 | shadow | BSD-3-clause;GPL-1;GPL-2+;public-domain |
| perl | 5.36.0-7+deb12u4 | perl | Artistic;Artistic or GPL-1+ or Artistic-dist;Artistic-2;Artistic-dist;BSD-3-clause;BSD-3-clause-GENERIC;BSD-3-clause-with-weird-numbering;BSD-4-clause-POWERDOG;BZIP;CC0-1.0;DONT-CHANGE-THE-GPL;Expat;Expat or GPL-1+ or Artistic;GPL-1+;GPL-1+ or Artistic;GPL-1+ or Artistic or Artistic-dist;GPL-1+ or Artistic, and BSD-3-clause-GENERIC;GPL-1+ or Artistic, and BSD-4-clause-POWERDOG;GPL-1+ or Artistic, and Expat;GPL-1+ or Artistic, and Unicode;GPL-2+;GPL-2+ or Artistic;GPL-3+-WITH-BISON-EXCEPTION;HSIEH-BSD;HSIEH-DERIVATIVE;LGPL-2.1;REGCOMP;REGCOMP, and GPL-1+ or Artistic;RRA-KEEP-THIS-NOTICE;SDBM-PUBLIC-DOMAIN;TEXT-TABS;Unicode;ZLIB |
| perl-base | 5.36.0-7+deb12u4 | perl | Artistic;Artistic or GPL-1+ or Artistic-dist;Artistic-2;Artistic-dist;BSD-3-clause;BSD-3-clause-GENERIC;BSD-3-clause-with-weird-numbering;BSD-4-clause-POWERDOG;BZIP;CC0-1.0;DONT-CHANGE-THE-GPL;Expat;Expat or GPL-1+ or Artistic;GPL-1+;GPL-1+ or Artistic;GPL-1+ or Artistic or Artistic-dist;GPL-1+ or Artistic, and BSD-3-clause-GENERIC;GPL-1+ or Artistic, and BSD-4-clause-POWERDOG;GPL-1+ or Artistic, and Expat;GPL-1+ or Artistic, and Unicode;GPL-2+;GPL-2+ or Artistic;GPL-3+-WITH-BISON-EXCEPTION;HSIEH-BSD;HSIEH-DERIVATIVE;LGPL-2.1;REGCOMP;REGCOMP, and GPL-1+ or Artistic;RRA-KEEP-THIS-NOTICE;SDBM-PUBLIC-DOMAIN;TEXT-TABS;Unicode;ZLIB |
| perl-modules-5.36 | 5.36.0-7+deb12u4 | perl | Artistic;Artistic or GPL-1+ or Artistic-dist;Artistic-2;Artistic-dist;BSD-3-clause;BSD-3-clause-GENERIC;BSD-3-clause-with-weird-numbering;BSD-4-clause-POWERDOG;BZIP;CC0-1.0;DONT-CHANGE-THE-GPL;Expat;Expat or GPL-1+ or Artistic;GPL-1+;GPL-1+ or Artistic;GPL-1+ or Artistic or Artistic-dist;GPL-1+ or Artistic, and BSD-3-clause-GENERIC;GPL-1+ or Artistic, and BSD-4-clause-POWERDOG;GPL-1+ or Artistic, and Expat;GPL-1+ or Artistic, and Unicode;GPL-2+;GPL-2+ or Artistic;GPL-3+-WITH-BISON-EXCEPTION;HSIEH-BSD;HSIEH-DERIVATIVE;LGPL-2.1;REGCOMP;REGCOMP, and GPL-1+ or Artistic;RRA-KEEP-THIS-NOTICE;SDBM-PUBLIC-DOMAIN;TEXT-TABS;Unicode;ZLIB |
| python3 | 3.11.2-1+b1 | python3-defaults | (free-form copyright file) |
| python3-minimal | 3.11.2-1+b1 | python3-defaults | (free-form copyright file) |
| python3-pkg-resources | 66.1.1-1+deb12u2 | setuptools | Apache-2.0;BSD-3-clause |
| python3-six | 1.16.0-4 | six | Expat |
| python3.11 | 3.11.2-6+deb12u9 | python3.11 | * Permission to use this software in any way is granted without;By obtaining, using, and/or copying this software and/or its;Permission  is  hereby granted,  free  of charge,  to  any person;Permission is hereby granted, free of charge, to any person obtaining;Permission to use, copy, modify, and distribute this software and;Permission to use, copy, modify, and distribute this software and its;Permission to use, copy, modify, and distribute this software for any;Redistribution and use in source and binary forms, with or without;This software is provided 'as-is', without any express or implied;This software is provided as-is, without express or implied |
| python3.11-minimal | 3.11.2-6+deb12u9 | python3.11 | * Permission to use this software in any way is granted without;By obtaining, using, and/or copying this software and/or its;Permission  is  hereby granted,  free  of charge,  to  any person;Permission is hereby granted, free of charge, to any person obtaining;Permission to use, copy, modify, and distribute this software and;Permission to use, copy, modify, and distribute this software and its;Permission to use, copy, modify, and distribute this software for any;Redistribution and use in source and binary forms, with or without;This software is provided 'as-is', without any express or implied;This software is provided as-is, without express or implied |
| readline-common | 8.2-1.3 | readline | GFDL-NIV-1.3+;GPL-2+;GPL-3;GPL-3+;ISC-no-attribution |
| sed | 4.9-1+deb12u1 | sed | BSD-4-clause-UC;BSL-1;GFDL-NIV-1.3+;GPL-3+;GPL-3+ and ISC;ISC;X11;pcre |
| sysvinit-utils | 3.06-4 | sysvinit | GPL-2.0;GPL-2.0+;GPL-3.0 |
| tar | 1.34+dfsg-1.2+deb12u1 | tar | GPL-2+;GPL-3+;GPL-3+ with Bison exception;LGPL-3+ |
| terser | 5.16.5-2 | node-terser | BSD-2-clause;GPL-3+ |
| tzdata | 2026f-0+deb12u1 | tzdata | public-domain |
| usr-is-merged | 37~deb12u1 | usrmerge | GPL-2+ |
| util-linux | 2.38.1-5+deb12u3 | util-linux | BSD-3-clause;BSD-4-clause;BSLA;GPL-2;GPL-2+;GPL-3+;LGPL;LGPL-2+;LGPL-2.1+;LGPL-3+;MIT;public-domain |
| util-linux-extra | 2.38.1-5+deb12u3 | util-linux | BSD-3-clause;BSD-4-clause;BSLA;GPL-2;GPL-2+;GPL-3+;LGPL;LGPL-2+;LGPL-2.1+;LGPL-3+;MIT;public-domain |
| webpack | 5.75.0+dfsg+~cs17.16.14-1+deb12u1 | node-webpack | Expat;Zlib |
| zlib1g | 1:1.2.13.dfsg-1 | zlib | Zlib |

## 4. Shared libraries of Endora's programs

What each program of Endora's loads at run time (`ldd` in the image), and the Debian package that
owns it.

| Program | Library | Debian package |
|---|---|---|
| /usr/local/bin/endora-hq | libc.so.6 | libc6 |
| /usr/local/bin/endora-hq | libgcc_s.so.1 | libgcc-s1 |
| /usr/local/bin/endora-hq | libm.so.6 | libc6 |
| /usr/local/bin/endora-hq | the glibc dynamic loader | libc6 |
| /usr/local/bin/endora-node | libc.so.6 | libc6 |
| /usr/local/bin/endora-node | libgcc_s.so.1 | libgcc-s1 |
| /usr/local/bin/endora-node | libm.so.6 | libc6 |
| /usr/local/bin/endora-node | the glibc dynamic loader | libc6 |
| /usr/local/bin/endora-runner | libc.so.6 | libc6 |
| /usr/local/bin/endora-runner | libgcc_s.so.1 | libgcc-s1 |
| /usr/local/bin/endora-runner | the glibc dynamic loader | libc6 |
| /usr/local/bin/endora-worker | libc.so.6 | libc6 |
| /usr/local/bin/endora-worker | libgcc_s.so.1 | libgcc-s1 |
| /usr/local/bin/endora-worker | the glibc dynamic loader | libc6 |

## 5. Copyleft components and where their source is

The Debian source packages below have a GPL, LGPL, AGPL, MPL, EPL or CDDL licence in at least one
binary package's copyright file, found two ways: a `License:` line, or a scan of the whole text
(the Basis column; a free-form copyright file such as glibc's has no `License:` line, so only the
text scan finds it). The list is deliberately wide: it includes packages where the licence covers
only a part (a packaging script, a test, a bundled library). "Ships code" is a guess from the
installed file names (an executable in a `bin` directory, or a `.so` library), not from ELF headers.

| Source package | Version in the image | Binary packages in the image | Licences found | Basis | Ships code |
|---|---|---|---|---|---|
| acl | 2.3.1-3 | libacl1 | GPL, LGPL | licence line, text | yes |
| adduser | 3.134 | adduser | GPL | licence line, text | yes |
| apt | 2.6.2 | apt, libapt-pkg6.0 | GPL | licence line, text | yes |
| attr | 1:2.5.1-4 | libattr1 | GPL, LGPL | licence line, text | yes |
| audit | 1:3.0.9-1 | libaudit-common, libaudit1 | GPL, LGPL | licence line, text | yes |
| base-files | 12.4+deb12u15 | base-files | GPL | text | no |
| base-passwd | 3.6.1 | base-passwd | GPL | licence line, text | yes |
| bash | 5.2.15-2 | bash | GPL | licence line, text | yes |
| bzip2 | 1.0.8-5 | libbz2-1.0 | GPL | licence line, text | yes |
| c-ares | 1.18.1-3 | libc-ares2 | GPL | text | yes |
| ca-certificates | 20250419~deb12u2 | ca-certificates | AGPL, GPL, LGPL, MPL | licence line, text | yes |
| cdebconf | 0.270 | libdebconfclient0 | GPL | licence line, text | yes |
| coreutils | 9.1-1 | coreutils | GPL | licence line, text | yes |
| curl | 7.88.1-10+deb12u15 | libcurl3-gnutls | GPL | licence line, text | yes |
| cyrus-sasl2 | 2.1.28+dfsg-10+deb12u1 | libsasl2-2, libsasl2-modules-db | GPL | licence line, text | yes |
| dash | 0.5.12-2 | dash | GPL | licence line, text | yes |
| db5.3 | 5.3.28+dfsg2-1 | libdb5.3 | GPL | licence line, text | yes |
| debian-archive-keyring | 2023.3+deb12u2 | debian-archive-keyring | GPL | text | no |
| debianutils | 5.7-0.5~deb12u1 | debianutils | GPL | licence line, text | yes |
| diffutils | 1:3.8-4 | diffutils | GPL, LGPL | licence line, text | yes |
| dpkg | 1.21.23 | dpkg | GPL | licence line, text | yes |
| e2fsprogs | 1.47.0-2 | e2fsprogs, libcom-err2, libext2fs2, libss2, logsave | GPL, LGPL | licence line, text | yes |
| eslint | 6.4.0~dfsg+~6.1.9-7 | eslint | GPL | licence line, text | yes |
| findutils | 4.9.0-4 | findutils | GPL, LGPL | licence line, text | yes |
| gcc-12 | 12.2.0-14+deb12u1 | gcc-12-base, libgcc-s1, libstdc++6 | GPL, LGPL | text | yes |
| gdbm | 1.23-3 | libgdbm-compat4, libgdbm6 | GPL, LGPL | licence line, text | yes |
| git | 1:2.39.5-0+deb12u3 | git, git-man | GPL, LGPL | licence line, text | yes |
| glibc | 2.36-9+deb12u14 | libc-bin, libc6 | GPL, LGPL | text | yes |
| gmp | 2:6.2.1+dfsg1-1.1 | libgmp10 | GPL, LGPL | licence line, text | yes |
| gnupg2 | 2.2.40-1.1+deb12u2 | gpgv | GPL, LGPL | licence line, text | yes |
| gnutls28 | 3.7.9-2+deb12u7 | libgnutls30 | GPL, LGPL | licence line, text | yes |
| grep | 3.8-5 | grep | GPL | licence line, text | yes |
| gzip | 1.12-1 | gzip | GPL | licence line, text | yes |
| hostname | 3.23+nmu1 | hostname | GPL | licence line, text | yes |
| icu | 72.1-3+deb12u1 | libicu72 | GPL | licence line, text | yes |
| init-system-helpers | 1.65.2+deb12u1 | init-system-helpers | GPL | licence line, text | yes |
| keyutils | 1.6.3-2 | libkeyutils1 | GPL, LGPL | licence line, text | yes |
| krb5 | 1.20.1-2+deb12u5 | libgssapi-krb5-2, libk5crypto3, libkrb5-3, libkrb5support0 | GPL | text | yes |
| libcap-ng | 0.8.3-1 | libcap-ng0 | GPL, LGPL | licence line, text | yes |
| libcap2 | 1:2.66-4+deb12u3 | libcap2 | GPL | licence line, text | yes |
| liberror-perl | 0.17029-2 | liberror-perl | GPL | licence line, text | no |
| libffi | 3.4.4-1 | libffi8 | GPL, LGPL, MPL | licence line, text | yes |
| libgcrypt20 | 1.10.1-3+deb12u1 | libgcrypt20 | GPL, LGPL | text | yes |
| libgpg-error | 1.46-1 | libgpg-error0 | GPL, LGPL | licence line, text | yes |
| libidn2 | 2.3.3-1 | libidn2-0 | GPL, LGPL | licence line, text | yes |
| libnsl | 1.3.0-2 | libnsl2 | GPL, LGPL | licence line, text | yes |
| libseccomp | 2.5.4-1+deb12u1 | libseccomp2 | LGPL | licence line, text | yes |
| libselinux | 3.4-1 | libselinux1 | GPL, LGPL | text | yes |
| libsemanage | 3.4-1 | libsemanage-common, libsemanage2 | GPL, LGPL | text | yes |
| libsepol | 3.4-2.1 | libsepol2 | GPL, LGPL | licence line, text | yes |
| libtasn1-6 | 4.19.0-2+deb12u1 | libtasn1-6 | GPL, LGPL | text | yes |
| libtirpc | 1.3.3+ds-1 | libtirpc-common, libtirpc3 | GPL, LGPL | licence line, text | yes |
| libunistring | 1.0-2 | libunistring2 | GPL, LGPL | licence line, text | yes |
| libuv1 | 1.44.2-1+deb12u1 | libuv1, libuv1-dev | GPL | licence line, text | yes |
| libxcrypt | 1:4.4.33-2 | libcrypt1 | GPL, LGPL | text | yes |
| libzstd | 1.5.4+dfsg2-5 | libzstd1 | GPL | licence line, text | yes |
| lz4 | 1.9.4-1 | liblz4-1 | GPL | licence line, text | yes |
| mawk | 1.3.4.20200120-3.1 | mawk | GPL | licence line, text | yes |
| nettle | 3.8.1-2 | libhogweed6, libnettle8 | GPL, LGPL | licence line, text | yes |
| nghttp2 | 1.52.0-1+deb12u3 | libnghttp2-14 | GPL | licence line, text | yes |
| node-ansi-escapes | 5.0.0+really.4.3.1-1 | node-ansi-escapes | GPL | licence line, text | no |
| node-async | 0.8.0-6 | libjs-async, node-async | GPL | licence line, text | no |
| node-cliui | 7.0.4+repack+~cs3.1.0-4 | node-cliui | GPL | licence line, text | no |
| node-diff | 5.0.0~dfsg+~5.0.1-4 | node-diff | GPL | licence line, text | no |
| node-eslint-scope | 7.1.1+~3.7.4-1 | node-eslint-scope | GPL | licence line, text | no |
| node-eslint-utils | 3.0.0-3 | node-eslint-utils | GPL | licence line, text | no |
| node-eslint-visitor-keys | 3.3.0+~1.0.0-1 | node-eslint-visitor-keys | GPL | licence line, text | no |
| node-espree | 9.4.1~dfsg-1 | node-espree | GPL | licence line, text | no |
| node-esquery | 1.4.2~ds-1 | node-esquery | GPL | licence line, text | no |
| node-file-entry-cache | 6.0.0+~3.0.4+~2.0.0+~1.0.0+~2.0.1-5 | node-file-entry-cache, node-flat-cache, node-write | GPL | licence line, text | no |
| node-flatted | 3.2.7~ds-1+deb12u1 | node-flatted | GPL | licence line, text | no |
| node-functional-red-black-tree | 1.0.1+20181105-7 | node-functional-red-black-tree | GPL | licence line, text | no |
| node-glob | 8.0.3+~cs8.4.15-1 | node-glob | GPL | licence line, text | no |
| node-handlebars | 3:4.7.7+~4.1.0-1 | handlebars | GPL, LGPL | licence line, text | yes |
| node-ignore | 5.2.1-1 | node-ignore | GPL | licence line, text | no |
| node-json-schema | 0.4.0+~7.0.11-1 | node-json-schema | GPL | licence line, text | no |
| node-locate-path | 7.1.1-2 | node-locate-path | GPL | licence line, text | no |
| node-lodash | 4.17.21+dfsg+~cs8.31.198.20210220-9+deb12u1 | node-lodash, node-lodash-packages | GPL | licence line, text | no |
| node-mute-stream | 0.0.8+~0.0.1-3 | node-mute-stream | GPL | licence line, text | no |
| node-n3 | 1.16.3+~1.2.3+~1.10.4-1 | node-n3 | GPL | licence line, text | no |
| node-progress | 2.0.3-2 | node-progress | GPL | licence line, text | no |
| node-read | 1.0.7-5 | node-read | GPL | licence line, text | no |
| node-regexpp | 3.2.0-4+deb12u1 | node-regexpp | GPL | licence line, text | no |
| node-retry | 0.13.1+~0.12.1-1 | node-retry | GPL | licence line, text | no |
| node-serialize-javascript | 6.0.0-2+deb12u1 | node-serialize-javascript | GPL | licence line, text | no |
| node-string-width | 4.2.3+~cs13.2.3-1 | node-string-width | GPL | licence line, text | no |
| node-terser | 5.16.5-2 | node-terser, terser | GPL | licence line, text | yes |
| node-uri-js | 4.4.0+dfsg-8 | node-uri-js | GPL | licence line, text | no |
| node-uuid | 8.3.2+~8.3.3-3 | node-uuid | GPL | licence line, text | no |
| node-v8-compile-cache | 2.3.0-3+deb12u1 | node-v8-compile-cache | GPL | licence line, text | no |
| nodejs | 18.20.4+dfsg-1~deb12u3 | libnode-dev, libnode108, nodejs | AGPL, GPL, LGPL, MPL | licence line, text | yes |
| openldap | 2.5.13+dfsg-5 | libldap-2.5-0 | GPL | licence line, text | yes |
| openssl | 3.0.22-1~deb12u2 | libssl-dev, libssl3, openssl | GPL | licence line, text | yes |
| p11-kit | 0.24.1-2 | libp11-kit0 | LGPL | licence line, text | yes |
| pam | 1.5.2-6+deb12u2 | libpam-modules, libpam-modules-bin, libpam-runtime, libpam0g | GPL, LGPL | licence line, text | yes |
| perl | 5.36.0-7+deb12u4 | libperl5.36, perl, perl-base, perl-modules-5.36 | GPL, LGPL | licence line, text | yes |
| python3-defaults | 3.11.2-1 | libpython3-stdlib, python3, python3-minimal | GPL | text | yes |
| python3.11 | 3.11.2-6+deb12u9 | libpython3.11-minimal, libpython3.11-stdlib, python3.11, python3.11-minimal | GPL | text | yes |
| readline | 8.2-1.3 | libreadline8, readline-common | GPL, LGPL | licence line, text | yes |
| rtmpdump | 2.4+20151223.gitfa8646d.1-2 | librtmp1 | GPL, LGPL | text | yes |
| sed | 4.9-1+deb12u1 | sed | GPL, LGPL | licence line, text | yes |
| shadow | 1:4.13+dfsg1-1+deb12u2 | login, passwd | GPL | licence line, text | yes |
| sqlite3 | 3.40.1-2+deb12u2 | libsqlite3-0 | GPL | licence line, text | yes |
| systemd | 252.39-1~deb12u2 | libsystemd0, libudev1 | GPL, LGPL | licence line, text | yes |
| sysvinit | 3.06-4 | sysvinit-utils | GPL | licence line, text | yes |
| tar | 1.34+dfsg-1.2+deb12u1 | tar | GPL, LGPL | licence line, text | yes |
| usrmerge | 37~deb12u1 | usr-is-merged | GPL | licence line, text | no |
| util-linux | 2.38.1-5+deb12u3 | bsdutils, libblkid1, libmount1, libsmartcols1, libuuid1, mount, util-linux, util-linux-extra | GPL, LGPL | licence line, text | yes |
| xxhash | 0.8.1-1 | libxxhash0 | GPL | licence line, text | yes |
| xz-utils | 5.4.1-1+deb12u2 | liblzma5 | GPL, LGPL | licence line, text | yes |

### Where to get the source

Every Debian package above is unmodified, so its corresponding source is Debian's source package
at the version in the tables:

- the Debian archive: `https://deb.debian.org/debian/pool/main/` (the `.orig.tar.*`, `.debian.tar.*`
  and `.dsc` of that version), or `apt-get source <source-package>=<version>` on a Debian 12 host
  with a `deb-src` line;
- `https://snapshot.debian.org/` for the exact version once the archive has moved on;
- `https://sources.debian.org/` to browse it.

The Rust crates are not copyleft, but their sources are the crates.io packages `Cargo.lock` names,
and Endora's own source is this repository at the commit the image's `sha-<commit>` tag names.

### Written offer

DRAFT, NEEDS-OWNER-REVIEW: the wording and whether to give it are the owner's decision.

For any component above licensed under the GNU General Public License or the GNU Lesser General
Public License, Endora will give anyone who asks the complete corresponding source code, or the
exact Debian source files and version it was built from, on a medium customarily used for software
interchange (a download is enough), for a charge no more than the cost of doing so (none, for a
download), for three years from the last time we distributed an image that contains it.
To ask, open an issue at `https://github.com/RusticPotatos/endora-core/issues` naming the image
tag (or its digest) and the component. The offer is in addition to the Debian locations above,
which anyone can use without asking.

## 6. How this was made

- `sh scripts/gen-third-party-notices.sh <image> > THIRD-PARTY-NOTICES.md`, which runs itself in the
  image with no network: `/var/lib/dpkg/status` for the packages, each `/usr/share/doc/<package>/copyright`
  for the licences (`License:` lines and a text scan), `ldd` for the libraries, and
  `/usr/share/doc/endora-core/licenses/INDEX.tsv` (written by the Dockerfile's build stage from the
  crates `cargo tree` names) for the Rust crates.
- The Debian part is right for the image it was read from. The base images are pinned by digest, but
  the packages the last stage installs come from the live Debian archive when the image is built, so a
  later build can carry newer ones: regenerate this file for the release and commit it before the
  release build. The file reads the same from an amd64 and an arm64 build (the dynamic loader, whose
  file name names the CPU, is given by role).
