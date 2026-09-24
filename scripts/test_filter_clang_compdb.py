import importlib.util
import pathlib
import unittest


SPEC = importlib.util.spec_from_file_location(
    "filter_clang_compdb", pathlib.Path(__file__).with_name("filter-clang-compdb.py")
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ClangCompdbFilterTest(unittest.TestCase):
    def test_linux_nested_selection_preserves_commands(self):
        root = "/home/me/work, sample"
        commands = [
            {"directory": root, "file": "src/Keep.cpp", "arguments": ["clang++", "-I" + root + "/include", "src/Keep.cpp"]},
            {"directory": root, "file": "src/Skip.cpp", "command": 'clang++ -c "' + root + '/src/Skip.cpp"'},
            {"directory": root, "file": "nested/Keep.cpp", "command": 'clang++ -c "' + root + '/nested/Keep.cpp"'},
        ]
        result, unresolved = MODULE.transform(commands, root, {"src/Keep.cpp", "nested/Keep.cpp"})
        self.assertEqual(unresolved, 0)
        self.assertEqual([x["file"] for x in result], ["/workspace/src/Keep.cpp", "/workspace/nested/Keep.cpp"])
        self.assertEqual(result[0]["arguments"], ["clang++", "-I/workspace/include", "src/Keep.cpp"])
        self.assertEqual(result[1]["command"], 'clang++ -c "/workspace/nested/Keep.cpp"')
        self.assertEqual(commands[0]["directory"], root)

    def test_windows_drive_mixed_separators_spaces_and_unresolved(self):
        root = r"C:\Work, Sample"
        commands = [
            {"directory": root, "file": r"src\Keep.cpp", "command": 'clang++ -DKEEP=1 -c "C:\\Work, Sample\\src\\Keep.cpp"'},
            {"directory": "c:/work, sample", "file": "c:/work, sample/src/Keep.cpp", "arguments": ["clang++", "c:/work, sample/src/Keep.cpp"]},
            {"directory": "c:/work, sample", "file": "src/Skip.cpp", "arguments": ["clang++", "-IC:/Work, Sample/include", "src/Skip.cpp"]},
            {"directory": r"D:\Other", "file": r"D:\Other\unknown.cpp", "arguments": ["clang++", r"D:\Other\unknown.cpp"]},
        ]
        result, unresolved = MODULE.transform(commands, root, {"src/Keep.cpp"})
        self.assertEqual(unresolved, 1)
        self.assertEqual(len(result), 3)
        self.assertEqual(result[0]["directory"], "/workspace")
        self.assertEqual(result[0]["file"], "/workspace/src/Keep.cpp")
        self.assertEqual(result[0]["command"], 'clang++ -DKEEP=1 -c "/workspace/src/Keep.cpp"')
        self.assertEqual(result[1]["file"], "/workspace/src/Keep.cpp")
        self.assertEqual(result[1]["arguments"], ["clang++", "/workspace/src/Keep.cpp"])
        self.assertEqual(result[2]["file"], r"D:\Other\unknown.cpp")

    def test_macos_absolute_and_outside_root(self):
        root = "/Users/me/Project With Spaces"
        commands = [
            {"directory": root, "file": root + "/src/Keep.c", "command": 'clang -c "' + root + '/src/Keep.c"'},
            {"directory": root, "file": root + "/src/Skip.c", "command": 'clang -c "' + root + '/src/Skip.c"'},
            {"directory": "/tmp/generated", "file": "/tmp/generated/external.c", "command": "clang -c /tmp/generated/external.c"},
        ]
        result, unresolved = MODULE.transform(commands, root, {"src/Keep.c"})
        self.assertEqual(unresolved, 1)
        self.assertEqual([x["file"] for x in result], ["/workspace/src/Keep.c", "/tmp/generated/external.c"])
        self.assertEqual(result[0]["command"], 'clang -c "/workspace/src/Keep.c"')

    def test_embedded_root_requires_argument_boundary(self):
        root = "/home/me/project"
        command = "clang -I/tmp/home/me/project/include -I/home/me/project/include -DROOT=/home/me/project -F/home/me/project/Frameworks -include/home/me/project/prefix.h"
        self.assertEqual(
            MODULE.rewrite_embedded(command, root),
            "clang -I/tmp/home/me/project/include -I/workspace/include -DROOT=/workspace -F/workspace/Frameworks -include/workspace/prefix.h",
        )

    def test_windows_allowed_source_matches_case_insensitively(self):
        root = r"C:\Work"
        commands = [{"directory": root, "file": r"c:\work\src\keep.cpp", "arguments": ["clang++", r"c:\work\src\keep.cpp"], "command": r"clang++ -c c:\work\src\keep.cpp"}]
        result, unresolved = MODULE.transform(commands, root, {"Src/Keep.cpp"})
        self.assertEqual(unresolved, 0)
        self.assertEqual(result[0]["file"], "/workspace/Src/Keep.cpp")
        self.assertEqual(result[0]["arguments"], ["clang++", "/workspace/Src/Keep.cpp"])
        self.assertEqual(result[0]["command"], "clang++ -c /workspace/Src/Keep.cpp")

    def test_posix_shell_escaped_space(self):
        root = "/home/me/My Project"
        commands = [{"directory": root, "file": root + "/Some File.c", "command": r"clang -c /home/me/My\ Project/Some\ File.c"}]
        result, unresolved = MODULE.transform(commands, root, {"Some File.c"})
        self.assertEqual(unresolved, 0)
        self.assertEqual(result[0]["command"], r"clang -c /workspace/Some\ File.c")

    def test_windows_relative_source_include_and_directory_case(self):
        root = r"C:\Work"
        commands = [{
            "directory": r"c:\work\src",
            "file": "keep.cpp",
            "arguments": ["clang++", r"-Iinclude\common", "-c", "keep.cpp"],
            "command": r"clang++ -Iinclude\common -c keep.cpp",
        }]
        result, unresolved = MODULE.transform(commands, root, {"Src/Keep.cpp"})
        self.assertEqual(unresolved, 0)
        self.assertEqual(result[0]["directory"], "/workspace/Src")
        self.assertEqual(result[0]["file"], "/workspace/Src/Keep.cpp")
        self.assertEqual(result[0]["arguments"], ["clang++", "-Iinclude/common", "-c", "Keep.cpp"])
        self.assertEqual(result[0]["command"], "clang++ -Iinclude/common -c Keep.cpp")

    def test_windows_relative_source_from_root(self):
        root = r"C:\Work"
        commands = [{"directory": root, "file": r"src\keep.cpp", "arguments": ["clang++", "-c", r"src\keep.cpp"], "command": r"clang++ -c src\keep.cpp"}]
        result, unresolved = MODULE.transform(commands, root, {"src/keep.cpp"})
        self.assertEqual(unresolved, 0)
        self.assertEqual(result[0]["arguments"], ["clang++", "-c", "src/keep.cpp"])
        self.assertEqual(result[0]["command"], "clang++ -c src/keep.cpp")


if __name__ == "__main__":
    unittest.main()
