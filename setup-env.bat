call "C:\Program Files (x86)\Intel\oneAPI\setvars.bat"

call ".\vcpkg\bootstrap-vcpkg.bat"

.\vcpkg\vcpkg.exe integrate install
