/* geoip.rs
 *
 * Copyright 2026 Mark
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * SPDX-License-Identifier: MPL-2.0
 */

//! # Сервис резолвинга геолокации и метаданных узлов (GeoIP Service)
//!
//! Модуль отвечает за:
//! - Интеллектуальный эвристический парсинг названий профилей (включая цепочки серверов:
//!   `Россия - Нидерланды`, `RU -> DE`, эмодзи-флаги `🇳🇱`, `🇩🇪`, `🇺🇸`, ISO-коды и города)
//! - Сквозное определение Egress-локации через активный прокси-туннель (`resolve_egress_location`)
//! - Асинхронное определение страны и города по внешнему хосту (`resolve_host_location`) с fallback
//! - Форматирование геолокации в соответствии с требованиями UI (`"DE, Frankfurt"` или `"NL"`)

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Структурированная информация о географической локации и провайдере сервера
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GeoLocationInfo {
    /// Название страны (например, "Germany")
    pub country: String,
    /// Двухбуквенный ISO-код страны (например, "DE")
    pub country_code: String,
    /// Город (например, "Frankfurt")
    pub city: String,
    /// Провайдер или хостинг (например, "Hetzner Online GmbH")
    pub isp: String,
    /// Часовой пояс (например, "Europe/Berlin")
    pub timezone: String,
}

impl GeoLocationInfo {
    /// Форматирует отображаемую строку локации в виде "DE, Frankfurt" или "NL"
    pub fn format_display(&self) -> String {
        let code = self.country_code.trim().to_uppercase();
        let city = self.city.trim();

        if !code.is_empty() && !city.is_empty() {
            format!("{code}, {city}")
        } else if !code.is_empty() {
            code
        } else if !city.is_empty() {
            city.to_string()
        } else if !self.country.trim().is_empty() {
            self.country.trim().to_string()
        } else {
            "Unknown".to_string()
        }
    }
}

/// База известных стран и городов для эвристического парсинга названий профилей
struct CountryDef {
    code: &'static str,
    name_en: &'static str,
    keywords: &'static [&'static str],
    cities: &'static [(&'static str, &'static str)],
}

static COUNTRIES: &[CountryDef] = &[
    CountryDef {
        code: "NL",
        name_en: "Netherlands",
        keywords: &[
            "нидерланды",
            "нидерланд",
            "netherlands",
            "голландия",
            "голланд",
            "holland",
            "амстердам",
            "amsterdam",
            "rotterdam",
            "роттердам",
        ],
        cities: &[
            ("амстердам", "Amsterdam"),
            ("amsterdam", "Amsterdam"),
            ("роттердам", "Rotterdam"),
            ("rotterdam", "Rotterdam"),
        ],
    },
    CountryDef {
        code: "DE",
        name_en: "Germany",
        keywords: &[
            "германия",
            "герман",
            "germany",
            "deutschland",
            "доичланд",
            "франкфурт",
            "frankfurt",
            "берлин",
            "berlin",
            "дюссельдорф",
            "dusseldorf",
            "мюнхен",
            "munich",
        ],
        cities: &[
            ("франкфурт", "Frankfurt"),
            ("frankfurt", "Frankfurt"),
            ("берлин", "Berlin"),
            ("berlin", "Berlin"),
            ("дюссельдорф", "Dusseldorf"),
            ("dusseldorf", "Dusseldorf"),
            ("мюнхен", "Munich"),
            ("munich", "Munich"),
        ],
    },
    CountryDef {
        code: "US",
        name_en: "United States",
        keywords: &[
            "сша",
            "usa",
            "америка",
            "united states",
            "штаты",
            "нью-йорк",
            "new york",
            "майами",
            "miami",
            "лос-анджелес",
            "los angeles",
            "чикаго",
            "chicago",
            "сиэтл",
            "seattle",
            "даллас",
            "dallas",
            "атланта",
            "atlanta",
        ],
        cities: &[
            ("нью-йорк", "New York"),
            ("new york", "New York"),
            ("nyc", "New York"),
            ("майами", "Miami"),
            ("miami", "Miami"),
            ("лос-анджелес", "Los Angeles"),
            ("los angeles", "Los Angeles"),
            ("чикаго", "Chicago"),
            ("chicago", "Chicago"),
            ("сиэтл", "Seattle"),
            ("seattle", "Seattle"),
            ("даллас", "Dallas"),
            ("dallas", "Dallas"),
            ("атланта", "Atlanta"),
            ("atlanta", "Atlanta"),
        ],
    },
    CountryDef {
        code: "RU",
        name_en: "Russia",
        keywords: &[
            "россия",
            "росси",
            "russia",
            "рф",
            "москва",
            "moscow",
            "петербург",
            "питер",
            "spb",
            "новосибирск",
            "екатеринбург",
            "владивосток",
        ],
        cities: &[
            ("москва", "Moscow"),
            ("moscow", "Moscow"),
            ("петербург", "Saint Petersburg"),
            ("питер", "Saint Petersburg"),
            ("spb", "Saint Petersburg"),
            ("новосибирск", "Novosibirsk"),
            ("владивосток", "Vladivostok"),
        ],
    },
    CountryDef {
        code: "GB",
        name_en: "United Kingdom",
        keywords: &[
            "великобритания",
            "united kingdom",
            "британия",
            "англия",
            "uk",
            "britain",
            "england",
            "лондон",
            "london",
            "манчестер",
            "manchester",
        ],
        cities: &[
            ("лондон", "London"),
            ("london", "London"),
            ("манчестер", "Manchester"),
            ("manchester", "Manchester"),
        ],
    },
    CountryDef {
        code: "TR",
        name_en: "Turkey",
        keywords: &[
            "турция",
            "турци",
            "turkey",
            "стамбул",
            "istanbul",
            "анталья",
            "antalya",
            "измир",
            "izmir",
            "анкара",
            "ankara",
        ],
        cities: &[
            ("стамбул", "Istanbul"),
            ("istanbul", "Istanbul"),
            ("анталья", "Antalya"),
            ("antalya", "Antalya"),
            ("измир", "Izmir"),
            ("izmir", "Izmir"),
        ],
    },
    CountryDef {
        code: "FI",
        name_en: "Finland",
        keywords: &["финляндия", "финлянд", "finland", "хельсинки", "helsinki"],
        cities: &[("хельсинки", "Helsinki"), ("helsinki", "Helsinki")],
    },
    CountryDef {
        code: "SE",
        name_en: "Sweden",
        keywords: &["швеция", "швеци", "sweden", "стокгольм", "stockholm"],
        cities: &[("стокгольм", "Stockholm"), ("stockholm", "Stockholm")],
    },
    CountryDef {
        code: "FR",
        name_en: "France",
        keywords: &[
            "франция",
            "франци",
            "france",
            "париж",
            "paris",
            "марсель",
            "marseille",
        ],
        cities: &[("париж", "Paris"), ("paris", "Paris")],
    },
    CountryDef {
        code: "PL",
        name_en: "Poland",
        keywords: &[
            "польша",
            "польш",
            "poland",
            "варшава",
            "warsaw",
            "краков",
            "krakow",
        ],
        cities: &[("варшава", "Warsaw"), ("warsaw", "Warsaw")],
    },
    CountryDef {
        code: "KZ",
        name_en: "Kazakhstan",
        keywords: &[
            "казахстан",
            "kazakhstan",
            "алматы",
            "almaty",
            "астана",
            "astana",
        ],
        cities: &[
            ("алматы", "Almaty"),
            ("almaty", "Almaty"),
            ("астана", "Astana"),
            ("astana", "Astana"),
        ],
    },
    CountryDef {
        code: "SG",
        name_en: "Singapore",
        keywords: &["сингапур", "singapore"],
        cities: &[],
    },
    CountryDef {
        code: "JP",
        name_en: "Japan",
        keywords: &[
            "япония",
            "япони",
            "japan",
            "токио",
            "tokyo",
            "осака",
            "osaka",
        ],
        cities: &[
            ("токио", "Tokyo"),
            ("tokyo", "Tokyo"),
            ("осака", "Osaka"),
            ("osaka", "Osaka"),
        ],
    },
    CountryDef {
        code: "HK",
        name_en: "Hong Kong",
        keywords: &["гонконг", "hong kong", "hongkong"],
        cities: &[],
    },
    CountryDef {
        code: "UA",
        name_en: "Ukraine",
        keywords: &["украина", "украин", "ukraine", "киев", "kyiv", "kiev"],
        cities: &[("киев", "Kyiv"), ("kyiv", "Kyiv"), ("kiev", "Kyiv")],
    },
    CountryDef {
        code: "GE",
        name_en: "Georgia",
        keywords: &["грузия", "грузи", "georgia", "тбилиси", "tbilisi"],
        cities: &[("тбилиси", "Tbilisi"), ("tbilisi", "Tbilisi")],
    },
    CountryDef {
        code: "AM",
        name_en: "Armenia",
        keywords: &["армения", "армени", "armenia", "ереван", "yerevan"],
        cities: &[("ереван", "Yerevan"), ("yerevan", "Yerevan")],
    },
    CountryDef {
        code: "LV",
        name_en: "Latvia",
        keywords: &["латвия", "латви", "latvia", "рига", "riga"],
        cities: &[("рига", "Riga"), ("riga", "Riga")],
    },
    CountryDef {
        code: "EE",
        name_en: "Estonia",
        keywords: &["эстония", "эстони", "estonia", "таллин", "tallinn"],
        cities: &[("таллин", "Tallinn"), ("tallinn", "Tallinn")],
    },
    CountryDef {
        code: "LT",
        name_en: "Lithuania",
        keywords: &["литва", "литв", "lithuania", "вильнюс", "vilnius"],
        cities: &[("вильнюс", "Vilnius"), ("vilnius", "Vilnius")],
    },
    CountryDef {
        code: "CZ",
        name_en: "Czech Republic",
        keywords: &["чехия", "чехи", "czech", "прага", "prague"],
        cities: &[("прага", "Prague"), ("prague", "Prague")],
    },
    CountryDef {
        code: "AT",
        name_en: "Austria",
        keywords: &["австрия", "австри", "austria", "вена", "vienna"],
        cities: &[("вена", "Vienna"), ("vienna", "Vienna")],
    },
    CountryDef {
        code: "CH",
        name_en: "Switzerland",
        keywords: &[
            "швейцария",
            "швейцари",
            "switzerland",
            "цюрих",
            "zurich",
            "женева",
            "geneva",
        ],
        cities: &[
            ("цюрих", "Zurich"),
            ("zurich", "Zurich"),
            ("женева", "Geneva"),
            ("geneva", "Geneva"),
        ],
    },
    CountryDef {
        code: "ES",
        name_en: "Spain",
        keywords: &[
            "испания",
            "испани",
            "spain",
            "мадрид",
            "madrid",
            "барселона",
            "barcelona",
        ],
        cities: &[("мадрид", "Madrid"), ("madrid", "Madrid")],
    },
    CountryDef {
        code: "IT",
        name_en: "Italy",
        keywords: &["италия", "итали", "italy", "рим", "rome", "милан", "milan"],
        cities: &[
            ("рим", "Rome"),
            ("rome", "Rome"),
            ("милан", "Milan"),
            ("milan", "Milan"),
        ],
    },
    CountryDef {
        code: "CA",
        name_en: "Canada",
        keywords: &[
            "канада",
            "канад",
            "canada",
            "торонто",
            "toronto",
            "монреаль",
            "montreal",
            "ванкувер",
            "vancouver",
        ],
        cities: &[
            ("торонто", "Toronto"),
            ("toronto", "Toronto"),
            ("ванкувер", "Vancouver"),
            ("vancouver", "Vancouver"),
        ],
    },
    CountryDef {
        code: "AE",
        name_en: "United Arab Emirates",
        keywords: &["оаэ", "uae", "эмираты", "дубай", "dubai"],
        cities: &[("дубай", "Dubai"), ("dubai", "Dubai")],
    },
    CountryDef {
        code: "KR",
        name_en: "South Korea",
        keywords: &["корея", "коре", "korea", "сеул", "seoul"],
        cities: &[("сеул", "Seoul"), ("seoul", "Seoul")],
    },
    CountryDef {
        code: "TW",
        name_en: "Taiwan",
        keywords: &["тайвань", "taiwan", "тайбэй", "taipei"],
        cities: &[("тайбэй", "Taipei"), ("taipei", "Taipei")],
    },
    CountryDef {
        code: "CY",
        name_en: "Cyprus",
        keywords: &["кипр", "cyprus", "лимассол", "limassol"],
        cities: &[("лимассол", "Limassol"), ("limassol", "Limassol")],
    },
    CountryDef {
        code: "IN",
        name_en: "India",
        keywords: &[
            "индия",
            "инди",
            "india",
            "мумбаи",
            "mumbai",
            "дели",
            "delhi",
        ],
        cities: &[("мумбаи", "Mumbai"), ("дели", "Delhi")],
    },
    CountryDef {
        code: "BR",
        name_en: "Brazil",
        keywords: &["бразилия", "бразили", "brazil", "сан-паулу", "sao paulo"],
        cities: &[("сан-паулу", "Sao Paulo")],
    },
    CountryDef {
        code: "AU",
        name_en: "Australia",
        keywords: &[
            "австралия",
            "австрали",
            "australia",
            "сидней",
            "sydney",
            "мельбурн",
            "melbourne",
        ],
        cities: &[("сидней", "Sydney"), ("мельбурн", "Melbourne")],
    },
    CountryDef {
        code: "RO",
        name_en: "Romania",
        keywords: &["румыния", "румыни", "romania", "бухарест", "bucharest"],
        cities: &[("бухарест", "Bucharest")],
    },
    CountryDef {
        code: "BG",
        name_en: "Bulgaria",
        keywords: &["болгария", "болгари", "bulgaria", "софия", "sofia"],
        cities: &[("софия", "Sofia")],
    },
    CountryDef {
        code: "RS",
        name_en: "Serbia",
        keywords: &["сербия", "серби", "serbia", "белград", "belgrade"],
        cities: &[("белград", "Belgrade")],
    },
    CountryDef {
        code: "MD",
        name_en: "Moldova",
        keywords: &[
            "молдова",
            "молдови",
            "молдавия",
            "moldova",
            "кишинев",
            "chisinau",
        ],
        cities: &[("кишинев", "Chisinau")],
    },
    CountryDef {
        code: "IS",
        name_en: "Iceland",
        keywords: &["исландия", "исланди", "iceland", "рейкьявик", "reykjavik"],
        cities: &[("рейкьявик", "Reykjavik")],
    },
    CountryDef {
        code: "NO",
        name_en: "Norway",
        keywords: &["норвегия", "норвеги", "norway", "осло", "oslo"],
        cities: &[("осло", "Oslo")],
    },
    CountryDef {
        code: "DK",
        name_en: "Denmark",
        keywords: &["дания", "дани", "denmark", "копенгаген", "copenhagen"],
        cities: &[("копенгаген", "Copenhagen")],
    },
    CountryDef {
        code: "IE",
        name_en: "Ireland",
        keywords: &["ирландия", "ирланди", "ireland", "дублин", "dublin"],
        cities: &[("дублин", "Dublin")],
    },
    CountryDef {
        code: "BE",
        name_en: "Belgium",
        keywords: &["бельгия", "бельги", "belgium", "брюссель", "brussels"],
        cities: &[("брюссель", "Brussels")],
    },
    CountryDef {
        code: "PT",
        name_en: "Portugal",
        keywords: &["португалия", "португали", "portugal", "лиссабон", "lisbon"],
        cities: &[("лиссабон", "Lisbon")],
    },
    CountryDef {
        code: "IL",
        name_en: "Israel",
        keywords: &["израиль", "israel", "тель-авив", "tel aviv"],
        cities: &[("тель-авив", "Tel Aviv")],
    },
];

/// Извлекает двухбуквенные ISO-коды стран из эмодзи-флагов Unicode Regional Indicator.
pub fn extract_flags(text: &str) -> Vec<String> {
    let mut flags = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i + 1 < chars.len() {
        let c1 = chars[i];
        let c2 = chars[i + 1];
        if ('\u{1F1E6}'..='\u{1F1FF}').contains(&c1) && ('\u{1F1E6}'..='\u{1F1FF}').contains(&c2) {
            let b1 = ((c1 as u32 - 0x1F1E6) as u8 + b'A') as char;
            let b2 = ((c2 as u32 - 0x1F1E6) as u8 + b'A') as char;
            flags.push(format!("{b1}{b2}"));
            i += 2;
        } else {
            i += 1;
        }
    }
    flags
}

/// Ищет совпадение по названию страны или городу в переданном тексте.
fn match_country_in_text(text: &str) -> Option<GeoLocationInfo> {
    let lower = text.to_lowercase();

    // 1. Поиск города (город дает максимальную точность)
    for c in COUNTRIES {
        for (city_kw, city_display) in c.cities {
            if lower.contains(city_kw) {
                return Some(GeoLocationInfo {
                    country: c.name_en.to_string(),
                    country_code: c.code.to_string(),
                    city: city_display.to_string(),
                    isp: String::new(),
                    timezone: String::new(),
                });
            }
        }
    }

    // 2. Поиск по ключевым словам страны
    for c in COUNTRIES {
        for kw in c.keywords {
            if lower.contains(kw) {
                return Some(GeoLocationInfo {
                    country: c.name_en.to_string(),
                    country_code: c.code.to_string(),
                    city: String::new(),
                    isp: String::new(),
                    timezone: String::new(),
                });
            }
        }
    }

    // 3. Поиск двухбуквенного ISO-кода (например, [NL], [DE], [US], (NL), NL_, NL-)
    let delimiters = [
        ' ', '[', ']', '(', ')', '{', '}', '-', '_', '/', '|', ':', '#',
    ];
    for word in text.split(&delimiters[..]) {
        let trimmed = word.trim();
        if trimmed.len() == 2 && trimmed.chars().all(|ch| ch.is_ascii_uppercase()) {
            if let Some(c) = COUNTRIES.iter().find(|c| c.code == trimmed) {
                return Some(GeoLocationInfo {
                    country: c.name_en.to_string(),
                    country_code: c.code.to_string(),
                    city: String::new(),
                    isp: String::new(),
                    timezone: String::new(),
                });
            }
        }
    }

    None
}

/// Выполняет интеллектуальный эвристический разбор названия профиля ключа.
///
/// Особое внимание уделяется цепочкам серверов (релеям):
/// В названиях вроде `Россия - Нидерланды`, `RU -> DE`, `[RU-NL] MyKey`
/// целевым сервером выхода (Exit Node) является **последний** узел цепочки.
pub fn parse_location_from_name(name: &str) -> Option<GeoLocationInfo> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1. Проверка эмодзи-флагов: если в названии цепочка флагов (например, 🇷🇺 -> 🇳🇱),
    // берем последний флаг — он обозначает точку выхода
    let flags = extract_flags(trimmed);
    if let Some(last_flag) = flags.last() {
        if let Some(c) = COUNTRIES.iter().find(|c| c.code == last_flag) {
            let mut info = GeoLocationInfo {
                country: c.name_en.to_string(),
                country_code: c.code.to_string(),
                city: String::new(),
                isp: String::new(),
                timezone: String::new(),
            };

            // Пытаемся также найти город в названии
            let lower = trimmed.to_lowercase();
            for (city_kw, city_display) in c.cities {
                if lower.contains(city_kw) {
                    info.city = city_display.to_string();
                    break;
                }
            }

            return Some(info);
        }
    }

    // 2. Разбиение на звенья цепочки релеев
    // Разделители: "->", "→", "=>", "⇒", ">>", " - ", " — ", " – ", " | ", " / ", "-", "/"
    let chain_separators = [
        "->", "->", "→", "=>", "⇒", ">>", " - ", " — ", " – ", " | ", " / ", "/", "-",
    ];

    let mut segments: Vec<&str> = Vec::new();
    let current_segment = trimmed;

    // Рекурсивно или последовательно делим строку по разделителям цепочек
    for sep in &chain_separators {
        if current_segment.contains(sep) {
            segments = current_segment
                .split(sep)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            break;
        }
    }

    if segments.len() > 1 {
        // Проверяем сегменты с конца к началу (последний сегмент — точка выхода)
        for seg in segments.iter().rev() {
            if let Some(info) = match_country_in_text(seg) {
                return Some(info);
            }
        }
    }

    // 3. Прямой поиск в цельной строке названия
    match_country_in_text(trimmed)
}

/// Проверяет, является ли хост локальным или приватным адресом.
pub fn is_private_or_local_host(host: &str) -> bool {
    let trimmed = host.trim().trim_start_matches('[').trim_end_matches(']');
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("localhost")
        || trimmed == "127.0.0.1"
        || trimmed == "0.0.0.0"
        || trimmed == "::1"
    {
        return true;
    }

    if let Ok(ip) = trimmed.parse::<std::net::IpAddr>() {
        match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_broadcast()
                    || v4.is_unspecified()
            }
            std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
        }
    } else {
        false
    }
}

/// Ответ от API ip-api.com
#[derive(Debug, Deserialize)]
struct IpApiResponse {
    status: Option<String>,
    country: Option<String>,
    #[serde(rename = "countryCode")]
    country_code: Option<String>,
    city: Option<String>,
    isp: Option<String>,
    timezone: Option<String>,
}

/// Ответ от резервного API ipwhois.app
#[derive(Debug, Deserialize)]
struct IpWhoisResponse {
    success: Option<bool>,
    country: Option<String>,
    country_code: Option<String>,
    city: Option<String>,
    isp: Option<String>,
    timezone: Option<String>,
}

/// Асинхронно запрашивает геолокацию узла по входному хосту/IP через внешний интернет (без прокси).
pub async fn resolve_host_location(host: &str) -> Option<GeoLocationInfo> {
    let clean_host = host
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();

    if is_private_or_local_host(&clean_host) {
        return None;
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .ok()?;

    // 1. Попытка через ip-api.com
    let primary_url = format!(
        "http://ip-api.com/json/{clean_host}?fields=status,message,country,countryCode,city,isp,timezone"
    );
    if let Ok(resp) = client.get(&primary_url).send().await {
        if resp.status().is_success() {
            if let Ok(data) = resp.json::<IpApiResponse>().await {
                if data.status.as_deref() == Some("success") {
                    let info = GeoLocationInfo {
                        country: data.country.unwrap_or_default(),
                        country_code: data.country_code.unwrap_or_default(),
                        city: data.city.unwrap_or_default(),
                        isp: data.isp.unwrap_or_default(),
                        timezone: data.timezone.unwrap_or_default(),
                    };
                    if !info.country_code.is_empty() || !info.country.is_empty() {
                        return Some(info);
                    }
                }
            }
        }
    }

    // 2. Резервная попытка через ipwhois.app
    let fallback_url = format!("https://ipwhois.app/json/{clean_host}");
    if let Ok(resp) = client.get(&fallback_url).send().await {
        if resp.status().is_success() {
            if let Ok(data) = resp.json::<IpWhoisResponse>().await {
                if data.success.unwrap_or(false) {
                    let info = GeoLocationInfo {
                        country: data.country.unwrap_or_default(),
                        country_code: data.country_code.unwrap_or_default(),
                        city: data.city.unwrap_or_default(),
                        isp: data.isp.unwrap_or_default(),
                        timezone: data.timezone.unwrap_or_default(),
                    };
                    if !info.country_code.is_empty() || !info.country.is_empty() {
                        return Some(info);
                    }
                }
            }
        }
    }

    None
}

/// Асинхронно определяет истинную точку выхода (Egress IP) через активный туннель прокси.
///
/// Когда VPN включен, запрос к сервису геолокации проходит через ядро sing-box и удаленный сервер,
/// благодаря чему возвращается 100% реальная страна и город выхода из туннеля (независимо от релеев).
pub async fn resolve_egress_location(socks_port: u16) -> Option<GeoLocationInfo> {
    let proxy_str = format!("socks5://127.0.0.1:{socks_port}");
    let proxy = reqwest::Proxy::all(&proxy_str).ok()?;

    let client = reqwest::Client::builder()
        .proxy(proxy)
        .timeout(Duration::from_secs(4))
        .build()
        .ok()?;

    let url = "http://ip-api.com/json/?fields=status,message,country,countryCode,city,isp,timezone";
    if let Ok(resp) = client.get(url).send().await {
        if resp.status().is_success() {
            if let Ok(data) = resp.json::<IpApiResponse>().await {
                if data.status.as_deref() == Some("success") {
                    let info = GeoLocationInfo {
                        country: data.country.unwrap_or_default(),
                        country_code: data.country_code.unwrap_or_default(),
                        city: data.city.unwrap_or_default(),
                        isp: data.isp.unwrap_or_default(),
                        timezone: data.timezone.unwrap_or_default(),
                    };
                    if !info.country_code.is_empty() || !info.country.is_empty() {
                        return Some(info);
                    }
                }
            }
        }
    }

    None
}

/// Интеллектуальный комбинированный резолвинг локации:
/// 1. Сначала анализирует название ключа (флаги, цепочки `RU -> NL`, города).
/// 2. Если в названии ничего не указано — использует GeoIP по входному хосту.
pub async fn smart_resolve_key_location(name: &str, host: &str) -> Option<GeoLocationInfo> {
    // Приоритет 1: Разбор названия (защищает от Anycast и цепочек серверов)
    if let Some(from_name) = parse_location_from_name(name) {
        return Some(from_name);
    }

    // Приоритет 2: Резолвинг входного хоста через GeoIP
    resolve_host_location(host).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_display() {
        let info = GeoLocationInfo {
            country: "Germany".to_string(),
            country_code: "DE".to_string(),
            city: "Frankfurt".to_string(),
            isp: "Hetzner".to_string(),
            timezone: "Europe/Berlin".to_string(),
        };
        assert_eq!(info.format_display(), "DE, Frankfurt");

        let info_no_city = GeoLocationInfo {
            country: "Netherlands".to_string(),
            country_code: "nl".to_string(),
            city: "".to_string(),
            isp: "".to_string(),
            timezone: "".to_string(),
        };
        assert_eq!(info_no_city.format_display(), "NL");

        let empty = GeoLocationInfo::default();
        assert_eq!(empty.format_display(), "Unknown");
    }

    #[test]
    fn test_chains_and_relays_parsing() {
        // Пример 1: Россия - нидерланды -> выход NL
        let res1 = parse_location_from_name("Россия - нидерланды").expect("Должен распознать NL");
        assert_eq!(res1.country_code, "NL");

        // Пример 2: Россия - германия -> выход DE
        let res2 = parse_location_from_name("Россия - германия").expect("Должен распознать DE");
        assert_eq!(res2.country_code, "DE");

        // Пример 3: Цепочка со стрелкой RU -> DE
        let res3 = parse_location_from_name("RU -> DE Frankfurt").expect("Должен распознать DE");
        assert_eq!(res3.country_code, "DE");
        assert_eq!(res3.city, "Frankfurt");
        assert_eq!(res3.format_display(), "DE, Frankfurt");

        // Пример 4: США в названии, даже если сервер Anycast
        let res4 = parse_location_from_name("США (Чикаго) Fast").expect("Должен распознать US");
        assert_eq!(res4.country_code, "US");
        assert_eq!(res4.city, "Chicago");
        assert_eq!(res4.format_display(), "US, Chicago");

        // Пример 5: Скобочный ISO код [RU-NL]
        let res5 = parse_location_from_name("[RU-NL] Premium VLESS").expect("Должен распознать NL");
        assert_eq!(res5.country_code, "NL");

        // Пример 6: Цепочка эмодзи-флагов 🇷🇺 ➡️ 🇳🇱
        let res6 = parse_location_from_name("🇷🇺 ➡️ 🇳🇱 Amsterdam").expect("Должен распознать NL");
        assert_eq!(res6.country_code, "NL");
        assert_eq!(res6.city, "Amsterdam");
        assert_eq!(res6.format_display(), "NL, Amsterdam");

        // Пример 7: Одиночный флаг 🇺🇸
        let res7 = parse_location_from_name("🇺🇸 Miami Ultra").expect("Должен распознать US");
        assert_eq!(res7.country_code, "US");
        assert_eq!(res7.city, "Miami");
    }

    #[test]
    fn test_private_local_host_detection() {
        assert!(is_private_or_local_host("127.0.0.1"));
        assert!(is_private_or_local_host("localhost"));
        assert!(is_private_or_local_host("192.168.1.1"));
        assert!(is_private_or_local_host("10.0.0.1"));
        assert!(is_private_or_local_host("::1"));

        assert!(!is_private_or_local_host("1.1.1.1"));
        assert!(!is_private_or_local_host("google.com"));
        assert!(!is_private_or_local_host("de.server.vpn"));
    }
}
