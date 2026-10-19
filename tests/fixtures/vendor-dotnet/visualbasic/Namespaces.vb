Namespace A.B
 Public Class Same
  Public Function Value() As Integer
   Return 42
  End Function
 End Class
End Namespace
Namespace C.B
 Public Class Same
  Public Function Value() As A.B.Same
   Return New A.B.Same()
  End Function
 End Class
End Namespace
