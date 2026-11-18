"""Round-trip a Perfworks STEP export through a real CAD kernel.

The Rust test suite can check that the file is syntactically valid part 21 and that its
topology is manifold, but it cannot tell you whether OpenCASCADE will actually sew the
faces into solids -- and that is the failure mode that matters, because a STEP file with
unsewn faces imports "successfully" and arrives as nothing.

Usage (requires FreeCAD, which ships the OCC kernel):

    tools/validate_step.sh

freecadcmd will not run a .py given as an argument -- it treats arguments as documents to
open -- so the wrapper feeds this file through `-c exec(...)` and passes the STEP path in
PERFWORKS_STEP. Exits non-zero if any solid fails to form, is open, or is invalid.
"""

import os
import sys
import Part

path = os.environ["PERFWORKS_STEP"]
shape = Part.Shape()
shape.read(path)

print("solids: %d  shells: %d  faces: %d" % (len(shape.Solids), len(shape.Shells), len(shape.Faces)))
bb = shape.BoundBox
print("bbox: %.2f x %.2f x %.2f mm" % (bb.XLength, bb.YLength, bb.ZLength))

bad = 0
if not shape.Solids:
    print("FAIL: no solids formed -- faces did not sew")
    bad += 1

for i, s in enumerate(shape.Solids):
    ok = s.isValid() and s.isClosed()
    print("  solid %2d: %7.2f x %7.2f x %6.2f  vol=%9.1f  faces=%2d  %s"
          % (i, s.BoundBox.XLength, s.BoundBox.YLength, s.BoundBox.ZLength,
             s.Volume, len(s.Faces), "ok" if ok else "BAD"))
    if not ok:
        bad += 1

# Every face must belong to a solid; a loose shell means something failed to sew.
loose = len(shape.Shells) - len(shape.Solids)
if loose:
    print("FAIL: %d shell(s) did not become solids" % loose)
    bad += 1

print("RESULT:", "PASS" if bad == 0 else "FAIL (%d problem(s))" % bad)
if bad:
    sys.exit(1)
