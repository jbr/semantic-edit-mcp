fn configure(builder: &mut Builder) {
    builder.retries(3); // the upstream default is 1
    builder.timeout(30);
    builder.build();
}
