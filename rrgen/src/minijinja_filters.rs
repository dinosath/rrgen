use cruet::string::pluralize;
use heck::{ToKebabCase, ToLowerCamelCase, ToPascalCase, ToSnakeCase};
use minijinja::Environment;
use fake::{Dummy, Fake, Faker};
use fake::faker::address::en::{*};
use fake::faker::*;
use fake::faker::barcode::en::{Isbn, Isbn10, Isbn13};
use fake::faker::chrono::en::{Date, DateTime, Time};
use fake::faker::company::en::{Bs, BsAdj, BsNoun, BsVerb, Buzzword, BuzzwordMiddle, BuzzwordTail, CatchPhrase, CompanyName, CompanySuffix, Industry, Profession};
use fake::faker::creditcard::en::{*};
use fake::faker::currency::en::*;
use fake::faker::filesystem::en::{*};
use fake::faker::finance::en::*;
use fake::faker::http::en::{RfcStatusCode, ValidStatusCode};
use fake::faker::internet::en::{*};
use fake::faker::job::en::*;
use fake::faker::lorem::en::*;
use fake::faker::name::en::*;
use fake::faker::phone_number::en::{CellNumber, PhoneNumber};

/// Registers all available filters for a given `Minijinja` environment.
pub fn register_all(env: &mut Environment) {
    env.add_filter("snake_case", snake_case);
    env.add_filter("camel_case", camel_case);
    env.add_filter("kebab_case", kebab_case);
    env.add_filter("pascal_case", pascal_case);
    env.add_filter("lower_camel_case", lower_camel_case);
    env.add_filter("plural", plural);
    env.add_function("fake", fake);
}

pub fn snake_case(value: String) -> String {
    value.to_snake_case()
}

pub fn camel_case(value: String) -> String {
    value.to_lower_camel_case()
}

pub fn kebab_case(value: String) -> String {
    value.to_kebab_case()
}

pub fn pascal_case(value: String) -> String {
    value.to_pascal_case()
}

pub fn lower_camel_case(value: String) -> String {
    value.to_lower_camel_case()
}

pub fn plural(value: String) -> String {
    pluralize::to_plural(&value)
}

pub fn fake(value: String) -> String {
    match value.as_str() {
        "CityPrefix" => CityPrefix().fake(),
        "CitySuffix"=> CitySuffix().fake(),
        "CityName"=> CityName().fake(),
        "CountryName"=> CountryName().fake(),
        "CountryCode"=> CountryCode().fake(),
        "StreetSuffix"=> StreetSuffix().fake(),
        "StreetName"=> StreetName().fake(),
        "TimeZone"=> TimeZone().fake(),
        "StateName"=> StateName().fake(),
        "StateAbbr"=> StateAbbr().fake(),
        "SecondaryAddressType"=> SecondaryAddressType().fake(),
        "SecondaryAddress"=> SecondaryAddress().fake(),
        "ZipCode"=> ZipCode().fake(),
        "PostCode"=> PostCode().fake(),
        "BuildingNumber"=> BuildingNumber().fake(),
        "Latitude"=> Latitude().fake(),
        "Longitude"=> Longitude().fake(),
        "Isbn"=> Isbn().fake(),
        "Isbn10"=> Isbn10().fake(),
        "Isbn13"=> Isbn13().fake(),
        "CreditCardNumber"=> CreditCardNumber().fake(),
        "CompanySuffix"=> CompanySuffix().fake(),
        "CompanyName"=> CompanyName().fake(),
        "Buzzword"=> Buzzword().fake(),
        "BuzzwordMiddle"=> BuzzwordMiddle().fake(),
        "BuzzwordTail"=> BuzzwordTail().fake(),
        "CatchPhrase"=> CatchPhrase().fake(),
        "BsVerb"=> BsVerb().fake(),
        "BsAdj"=> BsAdj().fake(),
        "BsNoun"=> BsNoun().fake(),
        "Bs"=> Bs().fake(),
        "Profession"=> Profession().fake(),
        "Industry"=> Industry().fake(),
        "FreeEmailProvider"=> FreeEmailProvider().fake(),
        "DomainSuffix"=> DomainSuffix().fake(),
        "FreeEmail"=> FreeEmail().fake(),
        "SafeEmail"=> SafeEmail().fake(),
        "Username"=> Username().fake(),
        "Password"=> Password(1..10).fake(),
        "IPv4"=> IPv4().fake(),
        "IPv6"=> IPv6().fake(),
        "IP"=> IP().fake(),
        "MACAddress"=> MACAddress().fake(),
        "UserAgent"=> UserAgent().fake(),
        "Seniority"=> Seniority().fake(),
        "Field"=> Field().fake(),
        "Position"=> Position().fake(),
        "Word"=> Word().fake(),
        "FirstName"=> FirstName().fake(),
        "LastName"=> LastName().fake(),
        "Title"=> job::en::Title().fake(),
        "Suffix"=> Suffix().fake(),
        "Name"=> Name().fake(),
        "NameWithTitle"=> NameWithTitle().fake(),
        "PhoneNumber"=> PhoneNumber().fake(),
        "CellNumber"=> CellNumber().fake(),
        "FilePath"=> FilePath().fake(),
        "FileName"=> FileName().fake(),
        "FileExtension"=> FileExtension().fake(),
        "DirPath"=> DirPath().fake(),
        "MimeType"=> MimeType().fake(),
        "Semver"=> Semver().fake(),
        "SemverStable"=> SemverStable().fake(),
        "SemverUnstable"=> SemverUnstable().fake(),
        "CurrencyCode"=> CurrencyCode().fake(),
        "CurrencyName"=> CurrencyName().fake(),
        "CurrencySymbol"=> CurrencySymbol().fake(),
        "Bic"=> Bic().fake(),
        "Isin"=> Isin().fake(),
        "Time"=> Time().fake(),
        "Date"=> Date().fake(),
        "DateTime"=> DateTime().fake(),
        "RfcStatusCode"=> RfcStatusCode().fake(),
        "ValidStatusCode"=> ValidStatusCode().fake(),
        _ => "".to_string()
    }
}
