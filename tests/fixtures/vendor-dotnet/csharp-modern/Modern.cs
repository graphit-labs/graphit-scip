namespace Modern;
/// <summary>C# 14 field-backed property with genuine implicit backing-field references.</summary>
public class Features
{
    public string Label { get; set => field = value.Trim(); } = "initial";
    public string Read() => "😀".Length + Label;
}
