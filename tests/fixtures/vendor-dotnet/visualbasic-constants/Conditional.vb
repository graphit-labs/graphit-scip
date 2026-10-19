Namespace Options
#If GRAPHIT_CUSTOM Then
    ''' <summary>The custom project constant selects this real class.</summary>
    Public Class CustomSelected
        Public Function Value() As String
            Return "custom"
        End Function
    End Class
#Else
    Public Class UnexpectedCustomAlternate
    End Class
#End If
#If _MyType = "Empty" Then
    ''' <summary>The SDK default constant selects this real class.</summary>
    Public Class SdkDefaultSelected
    End Class
#Else
    Public Class UnexpectedMyTemplateAlternate
    End Class
#End If
End Namespace
