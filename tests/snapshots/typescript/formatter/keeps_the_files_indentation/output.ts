export class Greeter {
  greet(name: string) {
    return "hello " + name;
  }

  farewell(name: string) {
    if (name) {
      return "goodbye " + name;
    }
    return "goodbye";
  }
}
