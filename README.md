# AKP03E

Własna obsługa kontrolera **Ajazz AKP03E** (rev. 2, USB `0300:3002`) dla Windows, napisana w Rust:
6 klawiszy z ekranami LCD, 3 przyciski i 3 pokrętła.

- **Panel w przeglądarce** (`http://127.0.0.1:7878`) – edycja profili, klawiszy, kolorów, ikon i akcji, podgląd
  na żywo naciśnięć i obrotów, przycisk „Testuj” przy każdej akcji.
- **Ikona w zasobniku** – otwarcie panelu, ponowne połączenie z kontrolerem, autostart z Windows, zamknięcie.
- **Profile** – klawisze LCD są osobne dla każdego profilu; przyciski A/B/C są wspólne; pokrętła są wspólne,
  ale każdy profil może nadpisać wybrane pokrętło.
- **Akcje**: uruchom aplikację/plik/URL, skrót klawiszowy, wpisanie tekstu (opcjonalnie z Enterem),
  klawisze multimedialne, głośność i wyciszenie systemu, głośność aktywnej aplikacji, przełączanie mikrofonu
  (ikona robi się czerwona po wyciszeniu), blokada komputera, przełączanie profili, wygaszanie ekranów.
- Po wybudzeniu komputera (także z Modern Standby) program sam łączy się ponownie z kontrolerem.

## Domyślne profile

| Profil | Klawisze |
|---|---|
| Praca | Mikrofon, Terminal, VS Code, Jira, Grafana, Zrzut ekranu |
| Media | Play/Pauza, Poprzedni, Następny, Mikrofon, Wycisz, Zablokuj |
| Claude | Akceptuj (Enter), Stop (Esc), Tryb (Shift+Tab), `/clear`, `/compact`, Nowy Claude (`wt.exe claude`) |

Pokrętła: lewe – głośność systemu, górne – głośność aktywnej aplikacji, prawe – poprzedni/następny utwór
(w profilu Claude: strzałki ↑/↓ i Enter). Przyciski: A/B – poprzedni/następny profil, C – wygaszanie ekranów.

Adresy Jiry i Grafany w profilu „Praca” to przykłady – zmień je w panelu.

## Budowanie i instalacja

Wymagania: Rust (toolchain `stable-x86_64-pc-windows-gnu`, ustawiony w `rust-toolchain.toml`)
i MinGW-w64 w `PATH` (np. `winget install BrechtSanders.WinLibs.POSIX.UCRT`).

```powershell
.\install.ps1
```

Skrypt buduje wersję release, kopiuje ją do `%LOCALAPPDATA%\Programs\akp03e\akp03e-panel.exe` i uruchamia.
Konfiguracja i log: `%APPDATA%\akp03e\config.toml`, `%APPDATA%\akp03e\akp03e.log`.

Do pracy nad kodem: `cargo run` – wersja debug ma okno konsoli z logami.

Pomocnicze skrypty:

- `status.ps1` – procesy, wpis autostartu, koniec logu i stan panelu,
- `reconnect.ps1` – ponowne połączenie z kontrolerem.

## Uwagi

- Program nie jest podpisany i wysyła naciśnięcia klawiszy, więc antywirus (np. Norton) może go usunąć albo
  uruchamiać w izolacji (zmiany konfiguracji wtedy nie zapisują się na dysk). Pomaga wyjątek dla folderu
  `%LOCALAPPDATA%\Programs\akp03e`.
- Skróty i wpisywany tekst trafiają do aktywnego okna. Okna uruchomione jako administrator ich nie przyjmą.

Protokół urządzenia: biblioteka [mirajazz](https://github.com/4ndv/mirajazz), mapowanie wejść za
[opendeck-akp03](https://github.com/4ndv/opendeck-akp03).
