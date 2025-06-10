# Instructions for AI assistants working on this project

* We are using git. Please commit code any time it successfully builds and it feels like we have
  reached a meaningful checkpoint, even if it is not a complete deployable work unit. After each
  commit, review the code to see if there are opportunities to refactor.
* Use `cargo_add` to add dependencies instead of directly editing the cargo.toml. That way we use
  the most current versions
* Pause during development frequently to confer with your collaborator. Before any change, describe
  the specific code changes you intend to make
* Please use the `rustdoc` tools available to you, and report any usability concerns or suggested
  improvements.
* Targeting anonymous items is very brittle. Only replaced named items such as functions. Ask for
  help when editing use blocks, they are difficult to target currently. When replacing functions,
  the preceding comment is not included in the replaced item.
* Design principles:
  - Avoid writing functions longer than 100 lines by breaking out behavior into smaller functions.
  - For maintainability, any time we notice we're making the same change in multiple places,
    consider extracting common behavior.
  - This software is exclusively intended for AI agent use. We do not need to support human users.
  - A guiding principle is that the user is never wrong. It is our responsibility to make the tool
    support user intention wherever possible, which means accomodating whatever usage patterns AI
    agents frequently use, instead of trying to shape usage to our design.
  - We lean on library code wherever possible and judicious, to reduce maintenance burden.
  - Within reason, we have no performance constraints in this tool. It is always preferable to
    perform a more expensive algorithmic operation in rust than to require our AI agent users to
    simulate that same operation.
  - It is valuable and helpful to question assumptions about specific implementation directions as
    we discover more through development. We can't always know the ultimate direction until we start
    making changes in some direction.


Please familiarize yourself with the project next. If you're going to perform a full recursive
directory listing, run cargo_clean first because the target directory can grow quite large.

