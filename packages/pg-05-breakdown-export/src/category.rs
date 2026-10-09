// SPDX-License-Identifier: MIT OR Apache-2.0
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Cast,
    BackgroundActors,
    Stunts,
    Vehicles,
    Props,
    Camera,
    SpecialEffects,
    Wardrobe,
    MakeupHair,
    Animals,
    AnimalWrangler,
    Music,
    Sound,
    ArtDepartment,
    SetDressing,
    Greenery,
    SpecialEquipment,
    Security,
    AdditionalLabor,
    VisualEffects,
    MechanicalEffects,
    Miscellaneous,
}
#[derive(Clone, Copy)]
pub(crate) struct FdxCategory {
    pub id: &'static str,
    pub name: &'static str,
    pub number: u8,
}
impl Category {
    pub(crate) fn fdx(self) -> FdxCategory {
        let (id, name, number) = match self {
            Self::Cast => ("01fc9642-84ff-4366-b37c-a3068dee57e8", "Cast Members", 2),
            Self::BackgroundActors => (
                "028a4e2b-b507-4d09-88ab-90e3edae9071",
                "Background Actors",
                3,
            ),
            Self::Stunts => ("0377dbe6-77a3-41af-bda8-86eb2468fdbf", "Stunts", 4),
            Self::Vehicles => ("04721a56-f54b-49c8-80ad-d53887d6b851", "Vehicles", 5),
            Self::Props => ("05c556eb-6bc1-4a3a-b09f-f8b5ba1b6afa", "Props", 6),
            Self::Camera => ("47b02ff1-5161-4137-b736-f36eebba7643", "Camera", 7),
            Self::SpecialEffects => ("069e18b8-2109-4f3d-94e7-d802027a60a8", "Special Effects", 8),
            Self::Wardrobe => ("0726fa85-1e65-4ab8-87de-bf21d09b01f0", "Wardrobe", 9),
            Self::MakeupHair => ("08ae1eef-32ce-415f-9a9b-0982d2453ec4", "Makeup/Hair", 10),
            Self::Animals => ("09cb0d1c-ce01-4f22-bb64-b5f2e6c491c6", "Animals", 11),
            Self::AnimalWrangler => (
                "0ae40617-cc7c-48e6-ae2b-5aaecc09986f",
                "Animal Wrangler",
                12,
            ),
            Self::Music => ("0b0b44c9-aa4b-4c40-88b1-d94472ad7a26", "Music", 13),
            Self::Sound => ("0ce7d308-096d-4603-8fe8-349f72cd89ff", "Sound", 15),
            Self::ArtDepartment => ("c86eae40-3b01-41c3-a7de-6859e6ec971d", "Art Department", 16),
            Self::SetDressing => ("0debb71b-5743-4c53-80cc-e17e841ce645", "Set Dressing", 17),
            Self::Greenery => ("0e7a8fc5-5441-4bad-a9bf-5ddd3fe51c69", "Greenery", 18),
            Self::SpecialEquipment => (
                "0ff5cda4-4d43-4cfe-940f-91380c46fdad",
                "Special Equipment",
                19,
            ),
            Self::Security => ("109d0eaa-0334-4823-ac0c-b44d3f209dc4", "Security", 20),
            Self::AdditionalLabor => (
                "1179a4b1-70ee-4011-b4a2-809a0af09e92",
                "Additional Labor",
                21,
            ),
            Self::VisualEffects => ("12ab0932-e3b9-4b4a-bcd0-3da1b4e61d5e", "Visual Effects", 23),
            Self::MechanicalEffects => (
                "135cc9d1-c4d5-4d00-83d9-571f584ea9cd",
                "Mechanical Effects",
                24,
            ),
            Self::Miscellaneous => ("ce04f547-f7ee-40c9-ab66-d95a0c98034e", "Miscellaneous", 25),
        };
        FdxCategory { id, name, number }
    }
}
pub(crate) const SYNOPSIS: FdxCategory = FdxCategory {
    id: "8e5e75c2-713b-47df-a75f-f12648b98ded",
    name: "Synopsis",
    number: 1,
};
pub(crate) const NOTES: FdxCategory = FdxCategory {
    id: "15b6f4fd-4e74-4ad8-9971-b239d88c2997",
    name: "Notes",
    number: 26,
};
pub(crate) const SCRIPT_DAY: FdxCategory = FdxCategory {
    id: "63c140da-ef2b-491a-b416-b46f461abb89",
    name: "Script Day",
    number: 27,
};
pub(crate) const UNIT: FdxCategory = FdxCategory {
    id: "849f1ebf-5507-4f33-bff6-3a5b4d73be14",
    name: "Unit",
    number: 28,
};
pub(crate) const LOCATION: FdxCategory = FdxCategory {
    id: "c5e89e4d-f83e-4c28-950c-92a63f1b5f26",
    name: "Location",
    number: 30,
};
