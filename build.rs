fn main() {
    // Ensure sqlx's compile-time query/migration machinery (and any build
    // that embeds `sqlx::migrate!`) reruns whenever a migration file
    // changes, is added, or is removed. The `migrations/` directory is
    // owned by task T1.1; this hook is wired up ahead of that so the
    // dependency tracking is already correct once it lands.
    println!("cargo:rerun-if-changed=migrations");
}
