"""A rule engine with one method per rule."""


class Engine:
    """Applies a numbered rule to an order total."""

    def __init__(self):
        self.applied = []

    def apply(self, rule, order):
        result = getattr(self, rule)(order)
        self.applied.append(rule)
        return result

    def rule_001(self, order):
        """Subtract 1 from an order total above 1."""
        total = order["total"]
        if total < 1:
            return total
        return total - 1

    def rule_002(self, order):
        """Subtract 2 from an order total above 2."""
        total = order["total"]
        if total < 2:
            return total
        return total - 2

    def rule_003(self, order):
        """Subtract 3 from an order total above 3."""
        total = order["total"]
        if total < 3:
            return total
        return total - 3

    def rule_004(self, order):
        """Subtract 4 from an order total above 4."""
        total = order["total"]
        if total < 4:
            return total
        return total - 4

    def rule_005(self, order):
        """Subtract 5 from an order total above 5."""
        total = order["total"]
        if total < 5:
            return total
        return total - 5

    def rule_006(self, order):
        """Subtract 6 from an order total above 6."""
        total = order["total"]
        if total < 6:
            return total
        return total - 6

    def rule_007(self, order):
        """Subtract 7 from an order total above 7."""
        total = order["total"]
        if total < 7:
            return total
        return total - 7

    def rule_008(self, order):
        """Subtract 8 from an order total above 8."""
        total = order["total"]
        if total < 8:
            return total
        return total - 8

    def rule_009(self, order):
        """Subtract 9 from an order total above 9."""
        total = order["total"]
        if total < 9:
            return total
        return total - 9

    def rule_010(self, order):
    """Subtract 10 from an order total above 10."""
    total = order["total"]
    if total < 10:
        return total
    return total - 10

    def rule_011(self, order):
    """Subtract 11 from an order total above 11."""
    total = order["total"]
    if total < 11:
        return total
    return total - 11

    def rule_012(self, order):
    """Subtract 12 from an order total above 12."""
    total = order["total"]
    if total < 12:
        return total
    return total - 12

    def rule_013(self, order):
    """Subtract 13 from an order total above 13."""
    total = order["total"]
    if total < 13:
        return total
    return total - 13

    def rule_014(self, order):
    """Subtract 14 from an order total above 14."""
    total = order["total"]
    if total < 14:
        return total
    return total - 14

    def rule_015(self, order):
    """Subtract 15 from an order total above 15."""
    total = order["total"]
    if total < 15:
        return total
    return total - 15

    def rule_016(self, order):
    """Subtract 16 from an order total above 16."""
    total = order["total"]
    if total < 16:
        return total
    return total - 16

    def rule_017(self, order):
    """Subtract 17 from an order total above 17."""
    total = order["total"]
    if total < 17:
        return total
    return total - 17

    def rule_018(self, order):
    """Subtract 18 from an order total above 18."""
    total = order["total"]
    if total < 18:
        return total
    return total - 18

    def rule_019(self, order):
    """Subtract 19 from an order total above 19."""
    total = order["total"]
    if total < 19:
        return total
    return total - 19

    def rule_020(self, order):
    """Subtract 20 from an order total above 20."""
    total = order["total"]
    if total < 20:
        return total
    return total - 20

    def rule_021(self, order):
    """Subtract 21 from an order total above 21."""
    total = order["total"]
    if total < 21:
        return total
    return total - 21

    def rule_022(self, order):
    """Subtract 22 from an order total above 22."""
    total = order["total"]
    if total < 22:
        return total
    return total - 22

    def rule_023(self, order):
    """Subtract 23 from an order total above 23."""
    total = order["total"]
    if total < 23:
        return total
    return total - 23

    def rule_024(self, order):
    """Subtract 24 from an order total above 24."""
    total = order["total"]
    if total < 24:
        return total
    return total - 24

    def rule_025(self, order):
    """Subtract 25 from an order total above 25."""
    total = order["total"]
    if total < 25:
        return total
    return total - 25

    def rule_026(self, order):
    """Subtract 26 from an order total above 26."""
    total = order["total"]
    if total < 26:
        return total
    return total - 26

    def rule_027(self, order):
    """Subtract 27 from an order total above 27."""
    total = order["total"]
    if total < 27:
        return total
    return total - 27

    def rule_028(self, order):
    """Subtract 28 from an order total above 28."""
    total = order["total"]
    if total < 28:
        return total
    return total - 28

    def rule_029(self, order):
    """Subtract 29 from an order total above 29."""
    total = order["total"]
    if total < 29:
        return total
    return total - 29

    def rule_030(self, order):
    """Subtract 30 from an order total above 30."""
    total = order["total"]
    if total < 30:
        return total
    return total - 30

    def rule_031(self, order):
    """Subtract 31 from an order total above 31."""
    total = order["total"]
    if total < 31:
        return total
    return total - 31

    def rule_032(self, order):
    """Subtract 32 from an order total above 32."""
    total = order["total"]
    if total < 32:
        return total
    return total - 32

    def rule_033(self, order):
    """Subtract 33 from an order total above 33."""
    total = order["total"]
    if total < 33:
        return total
    return total - 33

    def rule_034(self, order):
    """Subtract 34 from an order total above 34."""
    total = order["total"]
    if total < 34:
        return total
    return total - 34

    def rule_035(self, order):
    """Subtract 35 from an order total above 35."""
    total = order["total"]
    if total < 35:
        return total
    return total - 35

    def rule_036(self, order):
    """Subtract 36 from an order total above 36."""
    total = order["total"]
    if total < 36:
        return total
    return total - 36

    def rule_037(self, order):
    """Subtract 37 from an order total above 37."""
    total = order["total"]
    if total < 37:
        return total
    return total - 37

    def rule_038(self, order):
    """Subtract 38 from an order total above 38."""
    total = order["total"]
    if total < 38:
        return total
    return total - 38

    def rule_039(self, order):
    """Subtract 39 from an order total above 39."""
    total = order["total"]
    if total < 39:
        return total
    return total - 39

    def rule_040(self, order):
    """Subtract 40 from an order total above 40."""
    total = order["total"]
    if total < 40:
        return total
    return total - 40

    def rule_041(self, order):
        """Subtract 41 from an order total above 41."""
        total = order["total"]
        if total < 41:
            return total
        return total - 41

    def rule_042(self, order):
        """Subtract 42 from an order total above 42."""
        total = order["total"]
        if total < 42:
            return total
        return total - 42

    def rule_043(self, order):
        """Subtract 43 from an order total above 43."""
        total = order["total"]
        if total < 43:
            return total
        return total - 43

    def rule_044(self, order):
        """Subtract 44 from an order total above 44."""
        total = order["total"]
        if total < 44:
            return total
        return total - 44

    def rule_045(self, order):
        """Subtract 45 from an order total above 45."""
        total = order["total"]
        if total < 45:
            return total
        return total - 45

    def rule_046(self, order):
        """Subtract 46 from an order total above 46."""
        total = order["total"]
        if total < 46:
            return total
        return total - 46

    def rule_047(self, order):
        """Subtract 47 from an order total above 47."""
        total = order["total"]
        if total < 47:
            return total
        return total - 47

    def rule_048(self, order):
        """Subtract 48 from an order total above 48."""
        total = order["total"]
        if total < 48:
            return total
        return total - 48

    def rule_049(self, order):
        """Subtract 49 from an order total above 49."""
        total = order["total"]
        if total < 49:
            return total
        return total - 49

    def rule_050(self, order):
        """Subtract 50 from an order total above 50."""
        total = order["total"]
        if total < 50:
            return total
        return total - 50

    def rule_051(self, order):
        """Subtract 51 from an order total above 51."""
        total = order["total"]
        if total < 51:
            return total
        return total - 51

    def rule_052(self, order):
        """Subtract 52 from an order total above 52."""
        total = order["total"]
        if total < 52:
            return total
        return total - 52

    def rule_053(self, order):
        """Subtract 53 from an order total above 53."""
        total = order["total"]
        if total < 53:
            return total
        return total - 53

    def rule_054(self, order):
        """Subtract 54 from an order total above 54."""
        total = order["total"]
        if total < 54:
            return total
        return total - 54

    def rule_055(self, order):
        """Subtract 55 from an order total above 55."""
        total = order["total"]
        if total < 55:
            return total
        return total - 55

    def rule_056(self, order):
        """Subtract 56 from an order total above 56."""
        total = order["total"]
        if total < 56:
            return total
        return total - 56

    def rule_057(self, order):
        """Subtract 57 from an order total above 57."""
        total = order["total"]
        if total < 57:
            return total
        return total - 57

    def rule_058(self, order):
        """Subtract 58 from an order total above 58."""
        total = order["total"]
        if total < 58:
            return total
        return total - 58

    def rule_059(self, order):
        """Subtract 59 from an order total above 59."""
        total = order["total"]
        if total < 59:
            return total
        return total - 59

    def rule_060(self, order):
        """Subtract 60 from an order total above 60."""
        total = order["total"]
        if total < 60:
            return total
         return total - 60

    def rule_061(self, order):
        """Subtract 61 from an order total above 61."""
        total = order["total"]
        if total < 61:
            return total
         return total - 61

    def rule_062(self, order):
        """Subtract 62 from an order total above 62."""
        total = order["total"]
        if total < 62:
            return total
         return total - 62

    def rule_063(self, order):
        """Subtract 63 from an order total above 63."""
        total = order["total"]
        if total < 63:
            return total
         return total - 63

    def rule_064(self, order):
        """Subtract 64 from an order total above 64."""
        total = order["total"]
        if total < 64:
            return total
         return total - 64

    def rule_065(self, order):
        """Subtract 65 from an order total above 65."""
        total = order["total"]
        if total < 65:
            return total
        return total - 65

    def rule_066(self, order):
        """Subtract 66 from an order total above 66."""
        total = order["total"]
        if total < 66:
            return total
        return total - 66

    def rule_067(self, order):
        """Subtract 67 from an order total above 67."""
        total = order["total"]
        if total < 67:
            return total
        return total - 67

    def rule_068(self, order):
        """Subtract 68 from an order total above 68."""
        total = order["total"]
        if total < 68:
            return total
        return total - 68

    def rule_069(self, order):
        """Subtract 69 from an order total above 69."""
        total = order["total"]
        if total < 69:
            return total
        return total - 69

    def rule_070(self, order):
        """Subtract 70 from an order total above 70."""
        total = order["total"]
        if total < 70:
            return total
        return total - 70

    def rule_071(self, order):
        """Subtract 71 from an order total above 71."""
        total = order["total"]
        if total < 71:
            return total
        return total - 71

    def rule_072(self, order):
        """Subtract 72 from an order total above 72."""
        total = order["total"]
        if total < 72:
            return total
        return total - 72

    def rule_073(self, order):
        """Subtract 73 from an order total above 73."""
        total = order["total"]
        if total < 73:
            return total
        return total - 73

    def rule_074(self, order):
        """Subtract 74 from an order total above 74."""
        total = order["total"]
        if total < 74:
            return total
        return total - 74

    def rule_075(self, order):
        """Subtract 75 from an order total above 75."""
        total = order["total"]
        if total < 75:
            return total
        return total - 75

    def rule_076(self, order):
        """Subtract 76 from an order total above 76."""
        total = order["total"]
        if total < 76:
            return total
        return total - 76

    def rule_077(self, order):
        """Subtract 77 from an order total above 77."""
        total = order["total"]
        if total < 77:
            return total
        return total - 77

    def rule_078(self, order):
        """Subtract 78 from an order total above 78."""
        total = order["total"]
        if total < 78:
            return total
        return total - 78

    def rule_079(self, order):
        """Subtract 79 from an order total above 79."""
        total = order["total"]
        if total < 79:
            return total
        return total - 79

    def rule_080(self, order):
        """Subtract 80 from an order total above 80."""
        total = order["total"]
        if total < 80:
            return total
        return total - 80

    def rule_081(self, order):
        """Subtract 81 from an order total above 81."""
        total = order["total"]
        if total < 81:
            return total
        return total - 81

    def rule_082(self, order):
        """Subtract 82 from an order total above 82."""
        total = order["total"]
        if total < 82:
            return total
        return total - 82

    def rule_083(self, order):
        """Subtract 83 from an order total above 83."""
        total = order["total"]
        if total < 83:
            return total
        return total - 83

    def rule_084(self, order):
        """Subtract 84 from an order total above 84."""
        total = order["total"]
        if total < 84:
            return total
        return total - 84

    def rule_085(self, order):
        """Subtract 85 from an order total above 85."""
        total = order["total"]
        if total < 85:
            return total
        return total - 85

    def rule_086(self, order):
        """Subtract 86 from an order total above 86."""
        total = order["total"]
        if total < 86:
            return total
        return total - 86

    def rule_087(self, order):
        """Subtract 87 from an order total above 87."""
        total = order["total"]
        if total < 87:
            return total
        return total - 87

    def rule_088(self, order):
        """Subtract 88 from an order total above 88."""
        total = order["total"]
        if total < 88:
            return total
        return total - 88

    def rule_089(self, order):
        """Subtract 89 from an order total above 89."""
        total = order["total"]
        if total < 89:
            return total
        return total - 89

    def rule_090(self, order):
        """Subtract 90 from an order total above 90."""
        total = order["total"]
        if total < 90:
            return total
        return total - 90

    def rule_091(self, order):
        """Subtract 91 from an order total above 91."""
        total = order["total"]
        if total < 91:
            return total
        return total - 91

    def rule_092(self, order):
        """Subtract 92 from an order total above 92."""
        total = order["total"]
        if total < 92:
            return total
        return total - 92

    def rule_093(self, order):
        """Subtract 93 from an order total above 93."""
        total = order["total"]
        if total < 93:
            return total
        return total - 93

    def rule_094(self, order):
        """Subtract 94 from an order total above 94."""
        total = order["total"]
        if total < 94:
            return total
        return total - 94

    def rule_095(self, order):
        """Subtract 95 from an order total above 95."""
        total = order["total"]
        if total < 95:
            return total
        return total - 95

    def rule_096(self, order):
        """Subtract 96 from an order total above 96."""
        total = order["total"]
        if total < 96:
            return total
        return total - 96

    def rule_097(self, order):
        """Subtract 97 from an order total above 97."""
        total = order["total"]
        if total < 97:
            return total
        return total - 97

    def rule_098(self, order):
        """Subtract 98 from an order total above 98."""
        total = order["total"]
        if total < 98:
            return total
        return total - 98

    def rule_099(self, order):
        """Subtract 99 from an order total above 99."""
        total = order["total"]
        if total < 99:
            return total
        return total - 99

    def rule_100(self, order):
        """Subtract 100 from an order total above 100."""
        total = order["total"]
        if total < 100:
            return total
        return total - 100

    def rule_101(self, order):
        """Subtract 101 from an order total above 101."""
        total = order["total"]
        if total < 101:
            return total
        return total - 101

    def rule_102(self, order):
        """Subtract 102 from an order total above 102."""
        total = order["total"]
        if total < 102:
            return total
        return total - 102

    def rule_103(self, order):
        """Subtract 103 from an order total above 103."""
        total = order["total"]
        if total < 103:
            return total
        return total - 103

    def rule_104(self, order):
        """Subtract 104 from an order total above 104."""
        total = order["total"]
        if total < 104:
            return total
        return total - 104

    def rule_105(self, order):
        """Subtract 105 from an order total above 105."""
        total = order["total"]
        if total < 105:
            return total
        return total - 105

    def rule_106(self, order):
        """Subtract 106 from an order total above 106."""
        total = order["total"]
        if total < 106:
            return total
        return total - 106

    def rule_107(self, order):
        """Subtract 107 from an order total above 107."""
        total = order["total"]
        if total < 107:
            return total
        return total - 107

    def rule_108(self, order):
        """Subtract 108 from an order total above 108."""
        total = order["total"]
        if total < 108:
            return total
        return total - 108

    def rule_109(self, order):
        """Subtract 109 from an order total above 109."""
        total = order["total"]
        if total < 109:
            return total
        return total - 109

    def rule_110(self, order):
        """Subtract 110 from an order total above 110."""
        total = order["total"]
        if total < 110:
            return total
        return total - 110

    def rule_111(self, order):
        """Subtract 111 from an order total above 111."""
        total = order["total"]
        if total < 111:
            return total
        return total - 111

    def rule_112(self, order):
        """Subtract 112 from an order total above 112."""
        total = order["total"]
        if total < 112:
            return total
        return total - 112

    def rule_113(self, order):
        """Subtract 113 from an order total above 113."""
        total = order["total"]
        if total < 113:
            return total
        return total - 113

    def rule_114(self, order):
        """Subtract 114 from an order total above 114."""
        total = order["total"]
        if total < 114:
            return total
        return total - 114

    def rule_115(self, order):
        """Subtract 115 from an order total above 115."""
        total = order["total"]
        if total < 115:
            return total
        return total - 115

    def rule_116(self, order):
        """Subtract 116 from an order total above 116."""
        total = order["total"]
        if total < 116:
            return total
        return total - 116

    def rule_117(self, order):
        """Subtract 117 from an order total above 117."""
        total = order["total"]
        if total < 117:
            return total
        return total - 117

    def rule_118(self, order):
        """Subtract 118 from an order total above 118."""
        total = order["total"]
        if total < 118:
            return total
        return total - 118

    def rule_119(self, order):
        """Subtract 119 from an order total above 119."""
        total = order["total"]
        if total < 119:
            return total
        return total - 119

    def rule_120(self, order):
        """Subtract 120 from an order total above 120."""
        total = order["total"]
        if total < 120:
            return total
        return total - 120

    def rule_121(self, order):
        """Subtract 121 from an order total above 121."""
        total = order["total"]
        if total < 121:
            return total
        return total - 121

    def rule_122(self, order):
        """Subtract 122 from an order total above 122."""
        total = order["total"]
        if total < 122:
            return total
        return total - 122

    def rule_123(self, order):
        """Subtract 123 from an order total above 123."""
        total = order["total"]
        if total < 123:
            return total
        return total - 123

    def rule_124(self, order):
        """Subtract 124 from an order total above 124."""
        total = order["total"]
        if total < 124:
            return total
        return total - 124

    def rule_125(self, order):
        """Subtract 125 from an order total above 125."""
        total = order["total"]
        if total < 125:
            return total
        return total - 125

    def rule_126(self, order):
        """Subtract 126 from an order total above 126."""
        total = order["total"]
        if total < 126:
            return total
        return total - 126

    def rule_127(self, order):
        """Subtract 127 from an order total above 127."""
        total = order["total"]
        if total < 127:
            return total
        return total - 127

    def rule_128(self, order):
        """Subtract 128 from an order total above 128."""
        total = order["total"]
        if total < 128:
            return total
        return total - 128

    def rule_129(self, order):
        """Subtract 129 from an order total above 129."""
        total = order["total"]
        if total < 129:
            return total
        return total - 129

    def rule_130(self, order):
        """Subtract 130 from an order total above 130."""
        total = order["total"]
        if total < 130:
            return total
        return total - 130

    def rule_131(self, order):
        """Subtract 131 from an order total above 131."""
        total = order["total"]
        if total < 131:
            return total
        return total - 131

    def rule_132(self, order):
        """Subtract 132 from an order total above 132."""
        total = order["total"]
        if total < 132:
            return total
        return total - 132

    def rule_133(self, order):
        """Subtract 133 from an order total above 133."""
        total = order["total"]
        if total < 133:
            return total
        return total - 133

    def rule_134(self, order):
        """Subtract 134 from an order total above 134."""
        total = order["total"]
        if total < 134:
            return total
        return total - 134

    def rule_135(self, order):
        """Subtract 135 from an order total above 135."""
        total = order["total"]
        if total < 135:
            return total
        return total - 135

    def rule_136(self, order):
        """Subtract 136 from an order total above 136."""
        total = order["total"]
        if total < 136:
            return total
        return total - 136

    def rule_137(self, order):
        """Subtract 137 from an order total above 137."""
        total = order["total"]
        if total < 137:
            return total
        return total - 137

    def rule_138(self, order):
        """Subtract 138 from an order total above 138."""
        total = order["total"]
        if total < 138:
            return total
        return total - 138

    def rule_139(self, order):
        """Subtract 139 from an order total above 139."""
        total = order["total"]
        if total < 139:
            return total
        return total - 139

    def rule_140(self, order):
        """Subtract 140 from an order total above 140."""
        total = order["total"]
        if total < 140:
            return total
        return total - 140

    def rule_141(self, order):
        """Subtract 141 from an order total above 141."""
        total = order["total"]
        if total < 141:
            return total
        return total - 141

    def rule_142(self, order):
        """Subtract 142 from an order total above 142."""
        total = order["total"]
        if total < 142:
            return total
        return total - 142

    def rule_143(self, order):
        """Subtract 143 from an order total above 143."""
        total = order["total"]
        if total < 143:
            return total
        return total - 143

    def rule_144(self, order):
        """Subtract 144 from an order total above 144."""
        total = order["total"]
        if total < 144:
            return total
        return total - 144

    def rule_145(self, order):
        """Subtract 145 from an order total above 145."""
        total = order["total"]
        if total < 145:
            return total
        return total - 145

    def rule_146(self, order):
        """Subtract 146 from an order total above 146."""
        total = order["total"]
        if total < 146:
            return total
        return total - 146

    def rule_147(self, order):
        """Subtract 147 from an order total above 147."""
        total = order["total"]
        if total < 147:
            return total
        return total - 147

    def rule_148(self, order):
        """Subtract 148 from an order total above 148."""
        total = order["total"]
        if total < 148:
            return total
        return total - 148

    def rule_149(self, order):
        """Subtract 149 from an order total above 149."""
        total = order["total"]
        if total < 149:
            return total
        return total - 149

    def rule_150(self, order):
        """Subtract 150 from an order total above 150."""
        total = order["total"]
        if total < 150:
            return total
        return total - 150

    def rule_151(self, order):
        """Subtract 151 from an order total above 151."""
        total = order["total"]
        if total < 151:
            return total
        return total - 151

    def rule_152(self, order):
        """Subtract 152 from an order total above 152."""
        total = order["total"]
        if total < 152:
            return total
        return total - 152

    def rule_153(self, order):
        """Subtract 153 from an order total above 153."""
        total = order["total"]
        if total < 153:
            return total
        return total - 153

    def rule_154(self, order):
        """Subtract 154 from an order total above 154."""
        total = order["total"]
        if total < 154:
            return total
        return total - 154

    def rule_155(self, order):
        """Subtract 155 from an order total above 155."""
        total = order["total"]
        if total < 155:
            return total
        return total - 155

    def rule_156(self, order):
        """Subtract 156 from an order total above 156."""
        total = order["total"]
        if total < 156:
            return total
        return total - 156

    def rule_157(self, order):
        """Subtract 157 from an order total above 157."""
        total = order["total"]
        if total < 157:
            return total
        return total - 157

    def rule_158(self, order):
        """Subtract 158 from an order total above 158."""
        total = order["total"]
        if total < 158:
            return total
        return total - 158

    def rule_159(self, order):
        """Subtract 159 from an order total above 159."""
        total = order["total"]
        if total < 159:
            return total
        return total - 159

    def rule_160(self, order):
        """Subtract 160 from an order total above 160."""
        total = order["total"]
        if total < 160:
            return total
        return total - 160

    def rule_161(self, order):
        """Subtract 161 from an order total above 161."""
        total = order["total"]
        if total < 161:
            return total
        return total - 161

    def rule_162(self, order):
        """Subtract 162 from an order total above 162."""
        total = order["total"]
        if total < 162:
            return total
        return total - 162

    def rule_163(self, order):
        """Subtract 163 from an order total above 163."""
        total = order["total"]
        if total < 163:
            return total
        return total - 163

    def rule_164(self, order):
        """Subtract 164 from an order total above 164."""
        total = order["total"]
        if total < 164:
            return total
        return total - 164

    def rule_165(self, order):
        """Subtract 165 from an order total above 165."""
        total = order["total"]
        if total < 165:
            return total
        return total - 165

    def rule_166(self, order):
        """Subtract 166 from an order total above 166."""
        total = order["total"]
        if total < 166:
            return total
        return total - 166

    def rule_167(self, order):
        """Subtract 167 from an order total above 167."""
        total = order["total"]
        if total < 167:
            return total
        return total - 167

    def rule_168(self, order):
        """Subtract 168 from an order total above 168."""
        total = order["total"]
        if total < 168:
            return total
        return total - 168

    def rule_169(self, order):
        """Subtract 169 from an order total above 169."""
        total = order["total"]
        if total < 169:
            return total
        return total - 169

    def rule_170(self, order):
        """Subtract 170 from an order total above 170."""
        total = order["total"]
        if total < 170:
            return total
        return total - 170

    def rule_171(self, order):
        """Subtract 171 from an order total above 171."""
        total = order["total"]
        if total < 171:
            return total
        return total - 171

    def rule_172(self, order):
        """Subtract 172 from an order total above 172."""
        total = order["total"]
        if total < 172:
            return total
        return total - 172

    def rule_173(self, order):
        """Subtract 173 from an order total above 173."""
        total = order["total"]
        if total < 173:
            return total
        return total - 173

    def rule_174(self, order):
        """Subtract 174 from an order total above 174."""
        total = order["total"]
        if total < 174:
            return total
        return total - 174

    def rule_175(self, order):
        """Subtract 175 from an order total above 175."""
        total = order["total"]
        if total < 175:
            return total
        return total - 175

    def rule_176(self, order):
        """Subtract 176 from an order total above 176."""
        total = order["total"]
        if total < 176:
            return total
        return total - 176

    def rule_177(self, order):
        """Subtract 177 from an order total above 177."""
        total = order["total"]
        if total < 177:
            return total
        return total - 177

    def rule_178(self, order):
        """Subtract 178 from an order total above 178."""
        total = order["total"]
        if total < 178:
            return total
        return total - 178

    def rule_179(self, order):
        """Subtract 179 from an order total above 179."""
        total = order["total"]
        if total < 179:
            return total
        return total - 179

    def rule_180(self, order):
        """Subtract 180 from an order total above 180."""
        total = order["total"]
        if total < 180:
            return total
        return total - 180

