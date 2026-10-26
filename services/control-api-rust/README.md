# Nestbox Control API (Rust)

Die Control Plane der Appliance. Sie nimmt Wünsche an (Sandbox anlegen, starten,
Provider hinterlegen), speichert sie nachvollziehbar und übersetzt sie über einen
Adapter in echte Container. Die Web-GUI (SolidJS) spricht ausschließlich mit
diesem HTTP-Vertrag.

## In 5 Minuten durch den Code

| Frage | Datei |
|---|---|
| Wo startet der Service? | `src/main.rs` – Config lesen, Adapter wählen, Server starten |
| Was ist ein Endpunkt? | `src/api/*.rs` – ein Modul pro Ressource |
| Wo liegen die Regeln? | `src/services/*.rs` – ohne HTTP, ohne Docker |
| Was ist eine Sandbox? | `src/domain/sandbox.rs` – reine Datentypen |
| Wie wird gespeichert? | `src/state.rs` – `state.json` und `secrets.json` |
| Wie wird gestartet? | `src/runtime/` – `trait RuntimeAdapter` + Implementierungen |
| Wie entsteht die Doku? | `src/openapi.rs` – `utoipa` liest die Typen |

Der Datenfluss ist immer derselbe:

```
HTTP → api/ (Validierung, Status) → services/ (Regeln) → runtime/ (Ausführung)
```

## Starten

```bash
cargo run
```

Umgebungsvariablen:

| Variable | Default | Bedeutung |
|---|---|---|
| `NESTBOX_STATE_PATH` | `/data/state.json` | Soll-Zustand |
| `NESTBOX_SECRETS_PATH` | neben dem State | API-Keys, Rechte 0600 |
| `NESTBOX_RUNNER_URL` | `http://sandbox-runner:9000` | Runtime-Adapter |
| `NESTBOX_RUNTIME_NETWORK` | `nestbox-runtime` | Docker-Netz der Sandboxes |
| `NESTBOX_LISTEN_ADDR` | `0.0.0.0:8000` | Listen-Adresse |
| `RUST_LOG` | `info` | Log-Level |

## Tests

```bash
cargo test
```

Die Tests benutzen `runtime::fake::FakeRuntime` – sie brauchen weder Docker noch
Netzwerk. Abgedeckt sind: Sandbox-Lifecycle, Namenskonflikte, Provider ohne
Secret-Leck, Umgebungs-Injektion, Bindings, Topologie und der OpenAPI-Vertrag.

## Einen Endpunkt hinzufügen

Beispiel „Sandbox pausieren“:

1. **Typ** in `src/domain/` ergänzen (falls neuer Körper nötig).
2. **Regel** in `src/services/` schreiben (keine HTTP-Typen importieren).
3. **Route** in `src/api/` anlegen und in `src/api/mod.rs` registrieren.
4. **Doku**: Handler in `src/openapi.rs` unter `paths(...)` eintragen,
   neue Typen unter `components(schemas(...))`.
5. **Test** in `tests/` ergänzen.

## Ein Feld hinzufügen

1. Feld im Domain-Typ ergänzen (`#[serde(default)]`, wenn optional).
2. Wenn es in der Ausgabe erscheinen soll: fertig – `utoipa` übernimmt es.
3. Wenn es Eingabe ist: im `*Input`-Typ ergänzen und in der Regel prüfen.
4. Test anpassen; GUI-Feld ergänzen.

## Den Adapter austauschen

`src/runtime/mod.rs` definiert `trait RuntimeAdapter`. Implementierungen:

* `http_runner::HttpRunner` – spricht den Docker-Sandbox-Runner (heute)
* `fake::FakeRuntime` – Tests

Für OpenShell genügt eine neue Datei `openshell.rs` mit demselben Trait. In
`main.rs` wird dann statt `HttpRunner` der neue Adapter gewählt. API, Services,
GUI und OpenAPI bleiben unverändert.

## Fehler

Jede Fehlerantwort hat dieselbe Form:

```json
{"error": {"code": "sandbox.not_found", "message": "sandbox 'x' wurde nicht gefunden"}}
```

`code` ist stabil, `message` für Menschen. Neue Fälle gehören in
`src/error.rs`, damit Status und Code zentral bleiben.

## Secrets

API-Keys stehen **nie** in `state.json`, nie in einer Antwort und nie im Log.
Sie liegen in `secrets.json` (0600) und werden nur als Umgebungswerte an eine
Sandbox übergeben (`OPENAI_BASE_URL`, `OPENAI_MODEL`, `OPENAI_API_KEY`).

## Rust-Konzepte in diesem Projekt

| Konzept | Wo es vorkommt | Kurz gesagt |
|---|---|---|
| `struct` + `derive` | `src/domain/` | Datentyp, Serialisierung wird generiert |
| `Option<T>` | überall | „Wert oder keiner“ |
| `Result<T, E>` + `?` | Services | Fehler weiterreichen statt werfen |
| `async` / `.await` | Handler, Adapter | Warten ohne Thread zu blockieren |
| `trait` | `RuntimeAdapter` | Schnittstelle, mehrere Implementierungen |
| `Arc` + `RwLock` | `AppState` | geteilter Zustand, mehrere Leser |
| `Serialize`/`Deserialize` | JSON | Typen werden zu JSON und zurück |

Für dieses Projekt braucht man kein tieferes Rust-Wissen als diese sieben
Punkte. Alles Weitere ist Lesen von Dateien, nicht Sprachmagie.
