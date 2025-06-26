This is a confusing situation for both of us: I have several times sent the following message and some tool call you make as a result crashes claude desktop's UI consistently for me, prior to reaching my servers (there's nothing in the logs). Please pause and ask me to confirm between each tool call and explain what you're about to do, in order to try to figure out what's causing this intermittent bug that we seem to be able to reliably reproduce currently? My hunch is that the bug (in Claude Desktop) has something to do with calling a tool that doesn't exist, maybe? Below here is the message I sent several times that resulted in a Claude Desktop UI crash:

I realized I need to flag two known concerns that I intend to address, and this is very useful information to be accumulating:

1) I recently added the read interface to the filesystem tools, and now it's less obvious when to reach for the open_file tool within edit, but open_file offers additional AST structure beyond file contents, which is useful when performing later edits on a file. I need to rename open_file but have not yet done so
2) Session state is not shared across the two mcp servers yet, so you'll need to do a set_context call in the edit tools as well.

My apologies I didn't include either of these up front! Carry on!




Ohh interesting, that would absolutely do it. It is in fact open_files plural even if you're only opening a single file. Carry on, but before you open a file let me know if you intend to test the singular (which we'd expect would crash claude desktop and allow me to write an actionable bug report) or the plural (which would presumably allow us to proceed with our actual intention here)

It is in fact plural open_files, but that turned out not to be the problem. I'm doing a bit of a groundhog day loop here, because when claude desktop crashes we lose our last interaction pair, which is why pausing is useful. It seems to be an issue with the edit call, not the open_files call, so please pause for confirmation after your open_files (plural) call and describe exactly the edit operation — you're not doing anything wrong here, to be totally clear, and it's unlikely I am either. This is an unfortunate Claude Desktop bug and I can't even write a bug report other than "sometimes the screen goes blank and there's nothing in the logs"


Planned Edit Operation:
I want to add a new field to the User struct. Specifically, I want to add a phone_number field of type Option<String> to the field declaration list.
My plan is to use the edit:stage_operation tool with:

file_path: "example-usage/src/lib.rs"
selector:

anchor_text: "pub profile: UserProfile,"
ancestor_node_type: "field_declaration_list"
position: "before"


content: "pub phone_number: Option<String>,"

This should add the phone number field just before the profile field in the User struct.
Should I proceed with this edit operation?


I can't imagine why that would be a problem, but at least if this is in fact the issue I can try to write a confusing reproducer. Pause again after the edit call whether or not it was successful, since if you get a result either way that wasn't the bug
