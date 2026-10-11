# 025 The example's translation, in the learner's language

status: merged · rank: 5 · level: standard · type: feature · request: learn-pali (user, 2026-10-07 and again 2026-10-11, on the Vakkali example « alaṃ, vakkali, kiṃ te iminā pūtikāyena diṭṭhena? … »: « il serait bien d'avoir la traduction ») · split from 004 (deck pipeline), done first

## Summary
Checked 2026-10-11 against the PaliCanon API (`GET /v1/sutta/{uid}/{lang}`, token-gated; the token read from the keychain into the process, never printed): SN 22.87 has `pli` (ms), `en` (sujato), `fr` (christelle) segment-aligned by SuttaCentral keys (`sn22.87:3.1` …), `vi` (minh_chau) by paragraph only (`p24`); Dhp 116–128 has en (sujato), fr (hu), vi (phantuananh). The Vakkali example is `sn22.87:3.1–3.3`: en « Enough, Vakkali! Why would you want to see this rotting body? One who sees the teaching sees me. One who sees me sees the teaching. », fr « Assez, Vakkali ! Pourquoi vouloir voir ce corps pourrissant ? Celui qui voit l'enseignement me voit. Celui qui me voit, voit l'enseignement. ».
Decided:
- **The deck carries the translations** (built once, offline at run time): `build_deck` gains a step that, for each term's example, maps the DPD `source` to a SuttaCentral uid (`SN22.87` → `sn22.87`, `MN10` → `mn10`, `DHP127` → the vagga range uid holding verse 127 (a table of the Dhp ranges), `SNP…`, `TH…`/`THI…` → `thag`/`thig` …; an unmapped source is skipped and listed), fetches the `pli` text and each of `en`, `fr`, `vi`, finds the segments the example covers (normalised comparison: ṃ = ṁ, case, punctuation, quotes) and stores, per language, the joined translated segments and the translator. A translation that is not segment-aligned (keys not `uid:…`, e.g. vi minh_chau `p24`) is not stored for that language. Results are cached on disk (`build_deck` cache folder, outside git) so a rebuild does not re-fetch.
- **The token** comes from the environment (`PALI_API_TOKEN`), read from the keychain by the person running the build; `build_deck` never logs it; no token reaches the browser (the deck is a static file).
- **Deck format**: `Example` gains `translations: { lang: { text, translator } }` (optional, empty by default); `Deck::VERSION` stays readable for old decks (serde default), the fixture is regenerated with the translations.
- **The app shows it** under the Pāli line on the reveal (Context: under the filled line; Meaning: under 'Example'), in the selected language, English as the fallback, italic and muted, the translator after a dash (`— Bhikkhu Sujato`, names from the API's translator ids via a small table, unknown id shown as is); never on a Context front.
- **Credit**: the acknowledgements page lists SuttaCentral and the translators used (follow-up if the page is not the app's).
Rank 5: a network step in the pipeline (a new HTTP dependency, `cargo deny`), a deck format field, the app's reveal, licences to credit.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the API probed (sn22.87 pli/en/fr/vi, dhp116-128 en/fr/vi), the design written |
| Build | done | lane Opus: 9f9b87b core Example.translations, ae88124 build-deck from the API (curl with the token on stdin: ureq refused by cargo deny, webpki-roots CDLA), 66e461f fixture deck (en 49, fr 4, vi 9 of 50; 1 skipped; 65 language cases not segment-aligned), 705fbe9 the reveal shows it, 6d718c4 credit with a middle dot (copy rule); 472 workspace + 330 ssr tests; clippy ×3, fmt, rustdoc, leptosfmt, deny EXIT 0 |
| Capture | done | orchestrator, :3201 on the branch: a Context reveal (SN 10.5) shows the English translation in italics under the filled line, '· Bhikkhu Sujato', no console error |

## Merge
c36b726 (squash, 2026-10-11).

## Retro
The API was probed before the design, so the lane built on facts; still, the brief's alignment rule (uid prefix) was wrong for range uids, and its em dash broke the copy rule. Open: the acknowledgements credit (SuttaCentral and the translators); French and Vietnamese coverage is low (paragraph-keyed translations) — 004 decides whether to align them by paragraph.
