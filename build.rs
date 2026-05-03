fn main() {
    // Embed the Windows resource file (icon) into the executable.
    let _ = embed_resource::compile("resources.rc", embed_resource::NONE);
}
