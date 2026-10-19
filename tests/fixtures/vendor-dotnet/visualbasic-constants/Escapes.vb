Namespace Conditional
#If GRAPHIT_ESCAPED = "slash/percent%semi;quote""word" Then
    ''' <summary>The actual stock SDK compiler selects this escaped constant value.</summary>
    Public Class EscapedSdkSelected
    End Class
#Else
    Public Class UnexpectedEscapedAlternate
    End Class
#End If
End Namespace
