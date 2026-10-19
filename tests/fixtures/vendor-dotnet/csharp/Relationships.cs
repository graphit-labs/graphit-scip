namespace Relations;
public interface IValue { int Value(); }
public class Parent { public virtual int Value() => 1; }
public class Child : Parent, IValue { public override int Value() => base.Value(); }
public record RecordValue(int Value);
public class Generic<T> { public T Echo(T value) => value; }
public class Generic<T, U> { public U Echo(U value) => value; }
public class Uses { public RecordValue Record() => new RecordValue(42); public Generic<int> One() => new Generic<int>(); public Generic<int, string> Two() => new Generic<int, string>(); }
