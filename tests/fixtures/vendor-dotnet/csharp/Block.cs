namespace A.B { public class Same { public GlobalType Value() => new GlobalType(); } }
namespace C.B { public class Same { public A.B.Same Value() => new A.B.Same(); } }
