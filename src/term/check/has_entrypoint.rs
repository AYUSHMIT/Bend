use crate::{
  term::{Book, Definition, Name},
  ENTRY_POINT, HVM1_ENTRY_POINT,
};
use std::fmt::Display;

#[derive(Debug, Clone)]
pub enum EntryErr {
  NotFound(Name),
  Multiple(Vec<Name>),
  MultipleRules,
  Arguments,
}

impl Display for EntryErr {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      EntryErr::NotFound(name) => write!(f, "File has no '{name}' definition"),
      EntryErr::Multiple(fnd) if fnd.len() == 2 => {
        write!(f, "File has both '{}' and '{}' definitions", fnd[0], fnd[1])
      }
      EntryErr::Multiple(fnd) => {
        write!(f, "File has '{}', '{}' and '{}' definitions", fnd[0], fnd[1], fnd[2])
      }
      EntryErr::MultipleRules => write!(f, "Main definition can't have more than one rule"),
      EntryErr::Arguments => write!(f, "Main definition can't have any arguments"),
    }
  }
}

impl Book {
  pub fn check_has_entrypoint(&mut self) -> Option<Name> {
    let mut main = None;

    match self.get_possible_entry_points() {
      (Some(entry), None, None) | (None, Some(entry), None) | (None, None, Some(entry)) => {
        match self.validate_entry_point(entry) {
          Ok(name) => main = Some(name),
          Err(err) => self.info.error(err),
        }
      }

      (Some(a), Some(b), None) | (None, Some(a), Some(b)) | (Some(a), None, Some(b)) => {
        let entry = self.validate_entry_point(a);

        self.info.error(EntryErr::Multiple(vec![a.name.clone(), b.name.clone()]));

        match entry {
          Ok(name) => main = Some(name),
          Err(err) => self.info.error(err),
        }
      }

      (Some(a), Some(b), Some(c)) => {
        let entry = self.validate_entry_point(a);

        self.info.error(EntryErr::Multiple(vec![a.name.clone(), b.name.clone(), c.name.clone()]));

        match entry {
          Ok(name) => main = Some(name),
          Err(err) => self.info.error(err),
        }
      }

      (None, None, None) => {
        self.info.error(EntryErr::NotFound(self.entrypoint.clone().unwrap_or(Name::new(ENTRY_POINT))))
      }
    }

    main
  }

  fn validate_entry_point(&self, entry: &Definition) -> Result<Name, EntryErr> {
    if entry.rules.len() > 1 {
      Err(EntryErr::MultipleRules)
    } else if !entry.rules[0].pats.is_empty() {
      Err(EntryErr::Arguments)
    } else {
      Ok(entry.name.clone())
    }
  }

  fn get_possible_entry_points(&self) -> (Option<&Definition>, Option<&Definition>, Option<&Definition>) {
    let custom = self.entrypoint.as_ref().map(|e| self.defs.get(e)).flatten();
    let main = self.defs.get(&Name::new(ENTRY_POINT));
    let hvm1_main = self.defs.get(&Name::new(HVM1_ENTRY_POINT));
    (custom, main, hvm1_main)
  }
}
