//! Der Windows-Teil des stillen Drucks (ADR 0042).
//!
//! Dünn mit Absicht: Er fährt die Elementliste ab und übersetzt sie in
//! GDI-Aufrufe. Jede Entscheidung über das Aussehen fällt in
//! [`crate::tablet::blatt`], jede Umrechnung in [`super::Umrechnung`] —
//! beides ist prüfbar, ohne zu drucken.
//!
//! **Kein Pen, nur Rechtecke:** Linien und Rahmen entstehen als gefüllte
//! Rechtecke. Das spart die Pen-Verwaltung samt ihrer Handle-Lecks und
//! gibt exakte Strichstärken in Millimetern.

use super::{DruckFehler, Umrechnung};
use crate::tablet::blatt::{Ausrichtung, Element, Seite, TextKasten};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateDCW, CreateFontW, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, FillRect,
    GetDeviceCaps, SelectObject, SetBkMode, SetTextColor, ANSI_CHARSET, CLIP_DEFAULT_PRECIS,
    DEFAULT_PITCH, DEFAULT_QUALITY, DEVMODEW, DMORIENT_LANDSCAPE, DMPAPER_A4, DM_IN_BUFFER,
    DM_ORIENTATION, DM_OUT_BUFFER, DM_PAPERSIZE, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX,
    DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FF_SWISS, FW_BOLD, FW_NORMAL, HDC, HGDIOBJ, LOGPIXELSX,
    LOGPIXELSY, OUT_DEFAULT_PRECIS, PHYSICALOFFSETX, PHYSICALOFFSETY, TRANSPARENT,
};
use windows::Win32::Graphics::Printing::{
    ClosePrinter, DocumentPropertiesW, EnumPrintersW, GetDefaultPrinterW, OpenPrinterW,
    PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_HANDLE, PRINTER_INFO_4W,
};
// GDI-Druck aus gdi32.dll — im Binding unter `Storage::Xps` einsortiert.
use windows::Win32::Storage::Xps::{EndDoc, EndPage, StartDocW, StartPage, DOCINFOW};

/// Rust-Text → NUL-terminiertes UTF-16.
fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Die eingerichteten Drucker.
pub fn drucker_liste() -> Vec<String> {
    let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
    let mut gebraucht: u32 = 0;
    let mut anzahl: u32 = 0;
    unsafe {
        // Erster Aufruf ohne Puffer: Er scheitert erwartungsgemäß und
        // verrät dabei die nötige Größe.
        let _ = EnumPrintersW(flags, None, 4, None, &mut gebraucht, &mut anzahl);
        if gebraucht == 0 {
            return Vec::new();
        }
        let mut puffer = vec![0u8; gebraucht as usize];
        if EnumPrintersW(
            flags,
            None,
            4,
            Some(&mut puffer),
            &mut gebraucht,
            &mut anzahl,
        )
        .is_err()
        {
            return Vec::new();
        }
        let eintraege =
            std::slice::from_raw_parts(puffer.as_ptr() as *const PRINTER_INFO_4W, anzahl as usize);
        eintraege
            .iter()
            .filter_map(|e| e.pPrinterName.to_string().ok())
            .filter(|n| !n.is_empty())
            .collect()
    }
}

/// Der Windows-Standarddrucker, falls einer eingerichtet ist.
fn standarddrucker() -> Option<String> {
    let mut laenge: u32 = 0;
    unsafe {
        let _ = GetDefaultPrinterW(None, &mut laenge);
        if laenge == 0 {
            return None;
        }
        let mut puffer = vec![0u16; laenge as usize];
        if !GetDefaultPrinterW(Some(PWSTR(puffer.as_mut_ptr())), &mut laenge).as_bool() {
            return None;
        }
        let ende = puffer.iter().position(|&c| c == 0)?;
        let name = String::from_utf16_lossy(&puffer[..ende]);
        (!name.is_empty()).then_some(name)
    }
}

/// Welcher Drucker es sein soll: der eingestellte, sonst der
/// Windows-Standard.
fn ziel(drucker: &str) -> Result<String, DruckFehler> {
    let name = drucker.trim();
    if !name.is_empty() {
        return Ok(name.to_string());
    }
    standarddrucker().ok_or(DruckFehler::KeinDrucker)
}

/// Gerätekontext für **A4 quer** öffnen.
///
/// Über ein zusammengeführtes `DEVMODE`: Erst das Standardprofil des
/// Druckers holen, darin **nur Ausrichtung und Papiergröße** ändern, dann
/// zurückgeben lassen. Schacht, Auflösung und Farbeinstellung des
/// Anwenders bleiben damit erhalten.
unsafe fn kontext_quer(name: &str) -> Result<HDC, DruckFehler> {
    let breit = w(name);
    let mut drucker = PRINTER_HANDLE::default();
    if OpenPrinterW(PCWSTR(breit.as_ptr()), &mut drucker, None).is_err() {
        return Err(DruckFehler::NichtErreichbar(name.to_string()));
    }
    // Größe des DEVMODE erfragen. Ein negativer Wert heißt „geht nicht" —
    // dann drucken wir ohne eigenes Profil weiter (der Treiber nimmt sein
    // eigenes; das Blatt käme hochkant, aber ein Fehlschlag wäre schlimmer).
    let groesse = DocumentPropertiesW(None, drucker, PCWSTR(breit.as_ptr()), None, None, 0);
    let mut profil: Vec<u8> = Vec::new();
    let mut zeiger: *const DEVMODEW = std::ptr::null();
    if groesse > 0 {
        profil = vec![0u8; groesse as usize];
        let dm = profil.as_mut_ptr() as *mut DEVMODEW;
        let gelesen = DocumentPropertiesW(
            None,
            drucker,
            PCWSTR(breit.as_ptr()),
            Some(dm),
            None,
            DM_OUT_BUFFER.0,
        );
        if gelesen >= 0 {
            (*dm).Anonymous1.Anonymous1.dmOrientation = DMORIENT_LANDSCAPE as i16;
            // **Papierformat ausdrücklich auf A4.** Ohne diese Zeile nimmt
            // der Treiber seine Vorgabe — bei „Microsoft Print to PDF" auf
            // einem deutschen Windows gemessen: US-Letter. Letter quer ist
            // 279 mm breit, unser Raster braucht 275 mm plus Rand; die
            // letzte Spalte fiele also ab. Das zeigt sich erst am Papier,
            // deshalb steht es hier und nicht im Vertrauen auf den Treiber.
            (*dm).Anonymous1.Anonymous1.dmPaperSize = DMPAPER_A4 as i16;
            (*dm).dmFields |= DM_ORIENTATION | DM_PAPERSIZE;
            let vereint = DocumentPropertiesW(
                None,
                drucker,
                PCWSTR(breit.as_ptr()),
                Some(dm),
                Some(dm),
                DM_IN_BUFFER.0 | DM_OUT_BUFFER.0,
            );
            if vereint >= 0 {
                zeiger = dm;
            }
        }
    }
    let hdc = CreateDCW(None, PCWSTR(breit.as_ptr()), None, Some(zeiger));
    let _ = ClosePrinter(drucker);
    drop(profil);
    if hdc.is_invalid() {
        return Err(DruckFehler::NichtErreichbar(name.to_string()));
    }
    Ok(hdc)
}

/// Seiten drucken.
///
/// `datei` ist normalerweise `None`. Ist ein Pfad gesetzt, schreibt der
/// Treiber dorthin statt aufs Papier — das ist der Weg, den ganzen
/// Druckpfad (Querformat, Seitenfolge, Zeichnen) **ohne einen Baum**
/// nachzuweisen: gegen „Microsoft Print to PDF" mit Zieldatei entsteht
/// eine PDF, ganz ohne Speichern-Dialog.
pub fn drucke(seiten: &[Seite], titel: &str, drucker: &str) -> Result<(), DruckFehler> {
    drucke_nach(seiten, titel, drucker, None)
}

pub fn drucke_nach(
    seiten: &[Seite],
    titel: &str,
    drucker: &str,
    datei: Option<&str>,
) -> Result<(), DruckFehler> {
    let name = ziel(drucker)?;
    let ausgabe = datei.map(w);
    unsafe {
        let hdc = kontext_quer(&name)?;
        let umrechnung = Umrechnung {
            dpi_x: GetDeviceCaps(Some(hdc), LOGPIXELSX) as f32,
            dpi_y: GetDeviceCaps(Some(hdc), LOGPIXELSY) as f32,
            versatz_x: GetDeviceCaps(Some(hdc), PHYSICALOFFSETX) as f32,
            versatz_y: GetDeviceCaps(Some(hdc), PHYSICALOFFSETY) as f32,
        };
        let name_breit = w(titel);
        let info = DOCINFOW {
            cbSize: std::mem::size_of::<DOCINFOW>() as i32,
            lpszDocName: PCWSTR(name_breit.as_ptr()),
            lpszOutput: match &ausgabe {
                Some(pfad) => PCWSTR(pfad.as_ptr()),
                None => PCWSTR::null(),
            },
            ..Default::default()
        };
        if StartDocW(hdc, &info) <= 0 {
            let _ = DeleteDC(hdc);
            return Err(DruckFehler::Abgewiesen(
                "Der Spooler hat den Auftrag nicht angenommen.".into(),
            ));
        }
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(0x0000_0000));
        for seite in seiten {
            if StartPage(hdc) <= 0 {
                let _ = EndDoc(hdc);
                let _ = DeleteDC(hdc);
                return Err(DruckFehler::Abgewiesen("Seitenbeginn misslang.".into()));
            }
            for el in &seite.elemente {
                zeichne(hdc, &umrechnung, el);
            }
            if EndPage(hdc) <= 0 {
                let _ = EndDoc(hdc);
                let _ = DeleteDC(hdc);
                return Err(DruckFehler::Abgewiesen("Seitenende misslang.".into()));
            }
        }
        let ok = EndDoc(hdc) > 0;
        let _ = DeleteDC(hdc);
        if !ok {
            return Err(DruckFehler::Abgewiesen(
                "Der Auftrag wurde nicht abgeschlossen.".into(),
            ));
        }
    }
    Ok(())
}

/// Ein Element zeichnen.
unsafe fn zeichne(hdc: HDC, u: &Umrechnung, el: &Element) {
    match el {
        Element::Linie {
            x1,
            y1,
            x2,
            y2,
            staerke_mm,
        } => {
            let breite = (x2 - x1).abs().max(*staerke_mm);
            let hoehe = (y2 - y1).abs().max(*staerke_mm);
            flaeche(hdc, u, x1.min(*x2), y1.min(*y2), breite, hoehe, 0);
        }
        Element::Flaeche {
            x,
            y,
            breite,
            hoehe,
            grau,
        } => flaeche(hdc, u, *x, *y, *breite, *hoehe, *grau),
        Element::Rahmen {
            x,
            y,
            breite,
            hoehe,
            staerke_mm,
        } => {
            // Vier Striche statt eines Pens — gleiche Strichstärke oben wie
            // unten, unabhängig von der Rundung.
            flaeche(hdc, u, *x, *y, *breite, *staerke_mm, 0);
            flaeche(hdc, u, *x, y + hoehe - staerke_mm, *breite, *staerke_mm, 0);
            flaeche(hdc, u, *x, *y, *staerke_mm, *hoehe, 0);
            flaeche(hdc, u, x + breite - staerke_mm, *y, *staerke_mm, *hoehe, 0);
        }
        Element::Text(t) => text(hdc, u, t),
        // Das Logo bleibt dem HTML-Weg vorbehalten (Spec, Annahme zu E4):
        // Es zu laden hieße, einen Bild-Dekoder in den Druckpfad zu holen.
        // Der Platz bleibt frei, das Blatt gilt unverändert.
        Element::Logo { .. } => {}
    }
}

/// Gefülltes Rechteck in Graustufe (`0` = schwarz).
unsafe fn flaeche(hdc: HDC, u: &Umrechnung, x: f32, y: f32, breite: f32, hoehe: f32, grau: u8) {
    let rect = RECT {
        left: u.x(x),
        top: u.y(y),
        right: u.x(x) + u.laenge_x(breite).max(1),
        bottom: u.y(y) + u.laenge_y(hoehe).max(1),
    };
    let wert = grau as u32;
    let pinsel = CreateSolidBrush(COLORREF(wert | (wert << 8) | (wert << 16)));
    FillRect(hdc, &rect, pinsel);
    let _ = DeleteObject(HGDIOBJ(pinsel.0));
}

/// Text in seinen Kasten setzen: waagerecht nach Ausrichtung, senkrecht
/// mittig, zu lang wird gekürzt.
unsafe fn text(hdc: HDC, u: &Umrechnung, t: &TextKasten) {
    let hoehe = u.schrift(t.groesse_pt);
    let schriftart = w("Arial");
    let font = CreateFontW(
        -hoehe,
        0,
        0,
        0,
        if t.fett {
            FW_BOLD.0 as i32
        } else {
            FW_NORMAL.0 as i32
        },
        0,
        0,
        0,
        ANSI_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        DEFAULT_QUALITY,
        (DEFAULT_PITCH.0 | FF_SWISS.0) as u32,
        PCWSTR(schriftart.as_ptr()),
    );
    let vorher = SelectObject(hdc, HGDIOBJ(font.0));
    let mut rect = RECT {
        left: u.x(t.x),
        top: u.y(t.y),
        right: u.x(t.x) + u.laenge_x(t.breite),
        bottom: u.y(t.y) + u.laenge_y(t.hoehe),
    };
    let mut flags = DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX;
    flags |= match t.ausrichtung {
        Ausrichtung::Links => DT_LEFT,
        Ausrichtung::Mitte => DT_CENTER,
        Ausrichtung::Rechts => DT_RIGHT,
    };
    if t.kuerzen {
        flags |= DT_END_ELLIPSIS;
    }
    let mut inhalt: Vec<u16> = t.text.encode_utf16().collect();
    DrawTextW(hdc, &mut inhalt, &mut rect, flags);
    SelectObject(hdc, vorher);
    let _ = DeleteObject(HGDIOBJ(font.0));
}
