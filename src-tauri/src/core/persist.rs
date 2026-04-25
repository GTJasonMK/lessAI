pub(crate) fn save_and_return<T, Save>(value: T, save: Save) -> Result<T, String>
where
    Save: FnOnce(&T) -> Result<(), String>,
{
    let save_result = save(&value);
    finish_save(value, save_result)
}

pub(crate) fn maybe_save_and_return<T, Save>(
    value: T,
    should_save: bool,
    save: Save,
) -> Result<T, String>
where
    Save: FnOnce(&T) -> Result<(), String>,
{
    if !should_save {
        return Ok(value);
    }
    save_and_return(value, save)
}

fn finish_save<T>(value: T, save_result: Result<(), String>) -> Result<T, String> {
    save_result?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    #[test]
    fn save_and_return_persists_value_itself() {
        let save_calls = Cell::new(0);

        let value = super::save_and_return("value".to_string(), |saved| {
            save_calls.set(save_calls.get() + 1);
            assert_eq!(saved, "value");
            Ok(())
        })
        .expect("expected direct save to succeed");

        assert_eq!(value, "value");
        assert_eq!(save_calls.get(), 1);
    }

    #[test]
    fn maybe_save_and_return_skips_persist_when_not_requested() {
        let save_calls = Cell::new(0);

        let value = super::maybe_save_and_return("value".to_string(), false, |_| {
            save_calls.set(save_calls.get() + 1);
            Ok(())
        })
        .expect("expected conditional save to skip persist");

        assert_eq!(value, "value");
        assert_eq!(save_calls.get(), 0);
    }
}
