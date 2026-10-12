//! Moving items in a list (U13c): two neighbours swapped, each with what's written above it, in
//! a list of tables or an inline list.

use factional_content::{EditError, ValuePath, swap_items};

fn path(text: &str) -> ValuePath {
    ValuePath::parse(text).expect("a path")
}

const LINE: &str = "[line]\nname = \"Line\"\n\n# The first.\n[[line.steps]]\nquests = [\"a\"]\n\n# The second.\n[[line.steps]]\nquests = [\"b\", \"c\"]\nneed = 1\n\n[[line.steps]]\nquests = [\"d\"]\n";

#[test]
fn two_tables_of_a_list_swap_with_their_comments() {
    assert_eq!(
        swap_items(LINE, &path("line.steps"), 0, 1),
        Ok("[line]\nname = \"Line\"\n\n# The second.\n[[line.steps]]\nquests = [\"b\", \"c\"]\nneed = 1\n\n# The first.\n[[line.steps]]\nquests = [\"a\"]\n\n[[line.steps]]\nquests = [\"d\"]\n".to_owned())
    );
    // Either way round is the same swap.
    assert_eq!(
        swap_items(LINE, &path("line.steps"), 2, 1),
        Ok("[line]\nname = \"Line\"\n\n# The first.\n[[line.steps]]\nquests = [\"a\"]\n\n[[line.steps]]\nquests = [\"d\"]\n\n# The second.\n[[line.steps]]\nquests = [\"b\", \"c\"]\nneed = 1\n".to_owned())
    );
}

#[test]
fn two_items_of_an_inline_list_swap() {
    assert_eq!(
        swap_items(LINE, &path("line.steps[1].quests"), 0, 1),
        Ok(LINE.replace("[\"b\", \"c\"]", "[\"c\", \"b\"]"))
    );
}

#[test]
fn what_isnt_there_doesnt_swap() {
    assert_eq!(
        swap_items(LINE, &path("line.steps"), 1, 3),
        Err(EditError::NotThere(path("line.steps[3]")))
    );
    assert_eq!(
        swap_items(LINE, &path("line.name"), 0, 1),
        Err(EditError::NotThere(path("line.name")))
    );
}
