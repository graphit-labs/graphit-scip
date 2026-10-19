using SourceAlias = A.B.Same;
using ExternalAlias = System.Collections.Generic.List<int>;
namespace Aliases { public class Use { public SourceAlias Source() => new SourceAlias(); public ExternalAlias External() => new ExternalAlias(); } }
