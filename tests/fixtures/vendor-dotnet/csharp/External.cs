namespace ExternalUse;
public class Example { public string Value() => Newtonsoft.Json.JsonConvert.SerializeObject(System.DateTime.UtcNow); }
